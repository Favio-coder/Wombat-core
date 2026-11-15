#!/usr/bin/env python3
"""05_roads.py - metros de via por celda (Plan Maestro 6.8).

Entrada: grid.geojson del AOI, roads.geojson (OSM, grados) y changed.tif del par
         ya procesado por 03_change.py, mas road_buffer_meters de risk-config.
Salida:  road_cells.json en el formato que lee 06_cells.py (assemble.Inputs ->
         build_cells): metadatos del metodo y un mapa cell_id -> metros.

Hasta ahora este paso no existia para dato real: el unico que escribia
road_cells.json era synthetic.py. Sin el, 06_cells.py deja road_m_* en 0 con un
AVISO y Rust no los recalcula (wombat-risk lee cells.geojson tal cual), asi que
la exposicion vial -el insumo principal del ranking- quedaba vacia.

Metodo: las vias se reproyectan al CRS de la grilla (UTM, metros) y se recortan
celda por celda. road_m_total es la longitud recortada a la celda.
road_m_near_change es esa misma longitud intersecada con la union de los pixeles
changed == 1 dilatada road_buffer_meters. Metros enteros, redondeo medio hacia
arriba (R16: el payload firmado no admite flotantes).

El buffer se aplica al area del pixel, no a su centro: "a 250 m de un pixel con
cambio" significa lo que dice y no 250 m mas o menos medio pixel, que es lo que
daria una transformada de distancia sobre centros.

    python data-pipeline/validacion/05_roads.py --in <par> --inputs <insumos>
"""
from __future__ import annotations

import argparse
import json
import math
import sys
from decimal import Decimal, ROUND_HALF_UP
from pathlib import Path

import numpy as np
from pyproj import Transformer
from rasterio import features, windows
from shapely import STRtree
from shapely.geometry import LineString, box, shape
from shapely.ops import unary_union

sys.path.insert(0, str(Path(__file__).resolve().parent))
from wombat_nisar import raster_io  # noqa: E402
from wombat_nisar.grid import Grid  # noqa: E402

REPO = Path(__file__).resolve().parents[2]

#: Valor de changed.tif que cuenta como cambio. 0 = sin cambio, 255 = invalido;
#: un 255 no es cambio y no debe arrastrar vias al conteo de near_change.
CHANGED = 1

#: Segmentos por cuadrante al dilatar. Fijo y explicito: el valor por defecto de
#: shapely podria cambiar entre versiones y la salida tiene que ser reproducible.
QUAD_SEGS = 8

METHOD = ("vias recortadas a cada celda en el CRS de la grilla; near_change = "
          "interseccion con los pixeles changed==1 dilatados road_buffer_meters")


def _meters(value: float) -> int:
    """Metros enteros, redondeo medio hacia arriba.

    `round()` de Python redondea al par (round(0.5) == 0) y eso haria que dos
    celdas con la misma longitud cayeran a enteros distintos segun su paridad.
    """
    return int(Decimal(value).quantize(Decimal(1), rounding=ROUND_HALF_UP))


def _line_parts(geom: dict):
    """Las polilineas de una geometria GeoJSON; lo que no sea linea se ignora."""
    kind, coords = geom.get("type"), geom.get("coordinates") or []
    if kind == "LineString":
        return [coords]
    if kind == "MultiLineString":
        return list(coords)
    return []


def read_roads(path: Path, grid: Grid) -> list[LineString]:
    """roads.geojson reproyectado al CRS de la grilla (metros).

    GeoJSON es WGS84 por especificacion; si el archivo declara un miembro `crs`
    se respeta, como hace grid.from_geojson con la grilla.
    """
    fc = json.loads(path.read_text(encoding="utf-8"))
    code = (fc.get("crs") or {}).get("properties", {}).get("name", "").rsplit(":", 1)[-1]
    src = f"EPSG:{code}" if code.isdigit() else "EPSG:4326"
    to_grid = Transformer.from_crs(src, grid.crs, always_xy=True)
    lines = []
    for feat in fc.get("features", []):
        for part in _line_parts(feat.get("geometry") or {}):
            if len(part) < 2:
                continue
            xs, ys = to_grid.transform([p[0] for p in part], [p[1] for p in part])
            line = LineString(list(zip(xs, ys)))
            if line.length > 0:
                lines.append(line)
    return lines


def near_change_region(path: Path, grid: Grid, buffer_m: int, bounds):
    """Union de los pixeles con cambio, dilatada `buffer_m`. None si no hay ninguno.

    Solo se vectoriza la ventana que puede alcanzar a alguna via (su bbox inflado
    el buffer). En un AOI donde las vias ocupan una parte del raster eso evita
    poligonizar millones de pixeles que no cambian ningun resultado.
    """
    arr, transform, crs = raster_io.read(path)
    raster_io.require_compute_crs(crs, path.name, grid.crs)
    mask = arr == CHANGED
    if not mask.any():
        return None

    west, south, east, north = bounds
    inv = ~transform
    corners = [inv * (west - buffer_m, north + buffer_m),
               inv * (east + buffer_m, south - buffer_m)]
    cols, rows = [c for c, _ in corners], [r for _, r in corners]
    c0, c1 = max(0, math.floor(min(cols))), min(mask.shape[1], math.ceil(max(cols)))
    r0, r1 = max(0, math.floor(min(rows))), min(mask.shape[0], math.ceil(max(rows)))
    if c0 >= c1 or r0 >= r1:
        return None
    sub = mask[r0:r1, c0:c1]
    if not sub.any():
        return None

    window = windows.Window(c0, r0, c1 - c0, r1 - r0)
    win_transform = windows.transform(window, transform)
    polys = [shape(geom) for geom, _ in features.shapes(sub.astype(np.uint8), mask=sub,
                                                        transform=win_transform)]
    return unary_union(polys).buffer(buffer_m, quad_segs=QUAD_SEGS)


def road_cells(grid: Grid, lines: list[LineString], near) -> dict[str, dict[str, int]]:
    """cell_id -> {road_m_near_change, road_m_total}, en el orden de la grilla.

    `near` viene de unary_union(...).buffer(...), cuyos poligonos son disjuntos,
    asi que sumar la longitud intersecada con cada parte no cuenta nada dos veces.
    """
    tree = STRtree(lines) if lines else None
    near_parts = list(getattr(near, "geoms", [near])) if near is not None else []
    near_tree = STRtree(near_parts) if near_parts else None

    cells = {}
    for k in range(grid.n):
        cell = box(*grid.bounds(k))
        total = change = 0.0
        for i in (tree.query(cell) if tree is not None else ()):
            clipped = lines[i].intersection(cell)
            if clipped.is_empty:
                continue
            total += clipped.length
            for j in (near_tree.query(clipped) if near_tree is not None else ()):
                piece = clipped.intersection(near_parts[j])
                if not piece.is_empty:
                    change += piece.length
        if change > total + 1e-6:
            raise AssertionError(f"{grid.cell_id(k)}: near_change {change} > total {total}; "
                                 "la region dilatada no esta contenida en la celda")
        total_m = _meters(total)
        cells[grid.cell_id(k)] = {
            # El recorte garantiza near <= total en reales; el minimo solo cubre
            # el caso de dos redondeos que caen a lados distintos de un entero.
            "road_m_near_change": min(_meters(change), total_m),
            "road_m_total": total_m,
        }
    return cells


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--in", dest="in_dir", help="carpeta del par procesado (trae changed.tif)")
    ap.add_argument("--inputs", help="carpeta de insumos del AOI; da el valor por defecto de "
                                     "--grid, --roads y --out")
    ap.add_argument("--grid", help="por defecto <inputs>/grid.geojson")
    ap.add_argument("--roads", help="por defecto <inputs>/roads.geojson")
    ap.add_argument("--changed", help="por defecto <in>/changed.tif")
    ap.add_argument("--out", help="por defecto <inputs>/road_cells.json")
    ap.add_argument("--config", default=str(REPO / "config" / "risk-config.json"))
    ap.add_argument("--solo-total", action="store_true",
                    help="escribe road_m_total y deja road_m_near_change en 0. Solo para "
                         "inspeccionar las vias sin un par procesado: el archivo que sale NO "
                         "sirve para un bundle")
    args = ap.parse_args()

    inputs = Path(args.inputs) if args.inputs else None
    grid_path = Path(args.grid) if args.grid else (inputs / "grid.geojson" if inputs else None)
    roads_path = Path(args.roads) if args.roads else (inputs / "roads.geojson" if inputs else None)
    out_path = Path(args.out) if args.out else (inputs / "road_cells.json" if inputs else None)
    if not (grid_path and roads_path and out_path):
        print("ERROR: falta --inputs, o bien --grid, --roads y --out explicitos.")
        return 2
    for label, path in (("grid.geojson", grid_path), ("roads.geojson", roads_path)):
        if not path.exists():
            print(f"ERROR: no esta {label} en {path}.")
            return 2

    changed_path = Path(args.changed) if args.changed else (
        Path(args.in_dir) / "changed.tif" if args.in_dir else None)
    if not args.solo_total:
        # Sin changed.tif no hay near_change, y near_change en 0 no es un dato:
        # es el ranking sin su insumo principal. Se falla, no se rellena (R1).
        if changed_path is None:
            print("ERROR: falta changed.tif. Pasa --in <par> o --changed <ruta>.\n"
                  "       Lo produce 03_change.py. Para escribir solo road_m_total, "
                  "pide --solo-total de forma explicita.")
            return 2
        if not changed_path.exists():
            print(f"ERROR: no esta changed.tif en {changed_path}.\n"
                  "       Lo produce 03_change.py sobre los rasteres del par.")
            return 2

    grid = Grid.from_geojson(grid_path)
    buffer_m = int(json.loads(Path(args.config).read_text(encoding="utf-8"))["road_buffer_meters"])
    lines = read_roads(roads_path, grid)
    if not lines:
        print(f"AVISO: {roads_path} no tiene ninguna polilinea; todas las celdas quedan en 0.")

    near = None
    if not args.solo_total and lines:
        near = near_change_region(changed_path, grid, buffer_m, unary_union(lines).bounds)
        if near is None:
            print(f"AVISO: {changed_path.name} no marca cambio cerca de ninguna via; "
                  "road_m_near_change queda en 0 en todas las celdas.")

    cells = road_cells(grid, lines, near)
    doc = {
        "method": METHOD,
        "road_buffer_meters": buffer_m,
        "crs_compute": grid.crs,
        "roads": roads_path.name,
        "changed": None if args.solo_total else changed_path.name,
        "cells": cells,
    }
    if args.solo_total:
        doc["warning"] = ("road_m_near_change es 0 por --solo-total, no por ausencia de "
                          "cambio. No construyas un bundle con este archivo.")
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(doc, indent=1, ensure_ascii=False, allow_nan=False) + "\n",
                        encoding="utf-8")

    with_roads = [c for c in cells.values() if c["road_m_total"]]
    total_km = sum(c["road_m_total"] for c in cells.values()) / 1000
    near_km = sum(c["road_m_near_change"] for c in cells.values()) / 1000
    print(f"{out_path}: {len(cells)} celdas, {len(with_roads)} con via.")
    print(f"Via en la grilla: {total_km:.1f} km  |  a {buffer_m} m del cambio: {near_km:.1f} km")
    top = sorted(cells.items(), key=lambda kv: -kv[1]["road_m_near_change"])[:3]
    if near is not None:
        print("Mayor exposicion (solo referencia; el ranking lo calcula Rust, R15):")
        for cid, m in top:
            print(f"  {cid}  cerca={m['road_m_near_change']} m  de {m['road_m_total']} m")
    if args.solo_total:
        print("AVISO: --solo-total. road_m_near_change es 0 porque no se midio, no porque "
              "no haya cambio. Este archivo no sirve para un bundle.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
