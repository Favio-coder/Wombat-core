"""validate_opera.py - contrasta el cambio NISAR con OPERA DSWx-S1 (Plan Maestro §6.6).

OPERA DSWx-S1 es un producto NASA derivado de Sentinel-1, no de NISAR (R4). Aqui sirve como
fuente independiente: donde NISAR ve cambio, aparecio agua nueva segun OPERA?

Clases de la capa WTR (B01), confirmadas en la tabla de clases del producto y en los valores
observados en los archivos (R11): 0 no agua, 1 agua abierta, 3 vegetacion inundada,
250 enmascarado por HAND, 251 layover/sombra, 255 sin dato.
  https://developers.google.com/earth-engine/datasets/catalog/OPERA_DSWX_L3_V1_S1
  JPL D-108761 Rev A, OPERA DSWx-S1 Product Specification

Entrada: carpeta de un par NISAR ya procesado (changed.tif, manifest.json), su grid.geojson y
los GeoTIFF *_B01_WTR.tif de OPERA cercanos a t1 y a t2 (varios tiles por fecha si hace falta).
Salida: opera_cells.json en el formato que lee 06_cells.py, y un resumen de coincidencia.

Uso:
  python data-pipeline/validacion/validate_opera.py --in <par> --grid <grid.geojson> \
      --opera-t1 a_WTR.tif b_WTR.tif --opera-t2 c_WTR.tif d_WTR.tif --out <inputs>/opera_cells.json
Este script es para validar; la version de produccion de OPERA es de data-pipeline/osm_opera/.
"""
import argparse
import json
import re
import sys
from datetime import datetime
from pathlib import Path

import numpy as np
import rasterio
from rasterio.merge import merge
from rasterio.warp import Resampling, reproject

from wombat_nisar import raster_io
from wombat_nisar.grid import Grid

REPO = Path(__file__).resolve().parents[2]
WATER = (1, 3)          # agua abierta, vegetacion inundada
VALID = (0, 1, 3)       # 250, 251 y 255 no cuentan
_SENSING = re.compile(r"_(\d{8})T\d{6}Z_")


def _sensing_date(path: Path) -> datetime:
    return datetime.strptime(_SENSING.search(path.name).group(1), "%Y%m%d")


def _mosaic(paths, bounds, crs):
    sources = [rasterio.open(p) for p in paths]
    try:
        for src in sources:
            if src.crs != crs:
                raise ValueError(f"{Path(src.name).name} esta en {src.crs}, no en {crs}")
        data, transform = merge(sources, bounds=bounds, res=30, nodata=255)
    finally:
        for src in sources:
            src.close()
    return data[0], transform


def _count(idx, selector, n):
    chosen = idx[selector]
    return np.bincount(chosen[chosen >= 0], minlength=n)


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--in", dest="in_dir", required=True)
    ap.add_argument("--grid", required=True)
    ap.add_argument("--opera-t1", nargs="+", required=True)
    ap.add_argument("--opera-t2", nargs="+", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--config", default=str(REPO / "config" / "risk-config.json"))
    ap.add_argument("--allow-gap", action="store_true",
                    help="calcular aunque OPERA quede a mas dias de los permitidos (solo analisis)")
    args = ap.parse_args(argv)
    d = Path(args.in_dir)
    config = json.loads(Path(args.config).read_text(encoding="utf-8"))
    nisar = json.loads((d / "manifest.json").read_text(encoding="utf-8"))["nisar"]
    grid = Grid.from_geojson(args.grid)
    changed, c_transform, crs = raster_io.read(d / "changed.tif")
    bounds = (grid.x0, grid.y_top - grid.nrows * 1000, grid.x0 + grid.ncols * 1000, grid.y_top)

    sides, gaps = {}, {}
    for key, paths in (("t1", args.opera_t1), ("t2", args.opera_t2)):
        paths = [Path(p) for p in paths]
        dates = {_sensing_date(p) for p in paths}
        if len(dates) != 1:
            print(f"ERROR: los archivos OPERA de {key} son de fechas distintas.")
            return 2
        scene = datetime.strptime(nisar[key]["datetime"][:10], "%Y-%m-%d")
        gaps[key] = abs((dates.pop() - scene).days)
        sides[key] = {"granules": [p.name.replace("_B01_WTR.tif", "") for p in paths],
                      "gap_days": gaps[key]}
        sides[key]["wtr"], transform = _mosaic(paths, bounds, crs)
    too_far = max(gaps.values()) > config["opera_max_gap_days"]
    print(f"OPERA: {gaps['t1']} dia(s) de t1 y {gaps['t2']} dia(s) de t2 "
          f"(maximo permitido: {config['opera_max_gap_days']})")
    if too_far and not args.allow_gap:
        print("OPERA queda demasiado lejos: opera_status = unavailable (respuesta valida, §3.2).")

    w1, w2 = sides["t1"].pop("wtr"), sides["t2"].pop("wtr")
    both_valid = np.isin(w1, VALID) & np.isin(w2, VALID)
    water_t2 = np.isin(w2, WATER) & both_valid
    water_new = water_t2 & ~np.isin(w1, WATER)

    on_30 = np.full(w1.shape, 255, dtype=np.uint8)      # cambio NISAR llevado a la malla de 30 m
    reproject(changed, on_30, src_transform=c_transform, src_crs=crs, dst_transform=transform,
              dst_crs=crs, src_nodata=255, dst_nodata=255, resampling=Resampling.nearest)
    nisar_valid = (on_30 != 255) & both_valid
    changed_30 = (on_30 == 1) & both_valid

    idx = grid.index(transform, w1.shape)
    n_changed = _count(idx, changed_30, grid.n)
    n_both = _count(idx, changed_30 & water_new, grid.n)
    ratio = np.divide(n_both, n_changed, out=np.zeros(grid.n), where=n_changed > 0)
    threshold = config["corroboration_overlap_threshold"]
    cells = {}
    for k in range(grid.n):
        if too_far:
            cells[grid.cell_id(k)] = {"opera_status": "unavailable", "opera_overlap_ratio": None}
        else:
            r = round(float(ratio[k]), 4)
            cells[grid.cell_id(k)] = {
                "opera_status": "corroborated" if r >= threshold else "not_corroborated",
                "opera_overlap_ratio": r}

    area = lambda m: float(m.sum()) / max(int(nisar_valid.sum()), 1)
    stats = {
        "nisar_changed_fraction": round(area(changed_30), 4),
        "opera_new_water_fraction": round(area(water_new & nisar_valid), 4),
        "of_nisar_changed_is_opera_new_water": round(
            float((changed_30 & water_new).sum()) / max(int(changed_30.sum()), 1), 4),
        "of_nisar_changed_is_opera_water_at_t2": round(
            float((changed_30 & water_t2).sum()) / max(int(changed_30.sum()), 1), 4),
        "of_opera_new_water_is_nisar_changed": round(
            float((changed_30 & water_new).sum()) / max(int((water_new & nisar_valid).sum()), 1), 4),
        "of_nisar_unchanged_is_opera_new_water": round(
            float((nisar_valid & ~changed_30 & water_new).sum())
            / max(int((nisar_valid & ~changed_30).sum()), 1), 4),
    }
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps({
        "product": "OPERA_L3_DSWx-S1", "derived_from": "Sentinel-1",
        "t1": sides["t1"], "t2": sides["t2"], "stats": stats,
        "within_max_gap": not too_far, "cells": cells}, indent=1), encoding="utf-8")

    with_change = n_changed > 0
    corroborated = int(((ratio >= threshold) & with_change).sum())
    print(f"Cambio NISAR: {stats['nisar_changed_fraction']:.1%} del area | "
          f"agua nueva OPERA: {stats['opera_new_water_fraction']:.1%}")
    print(f"Del area con cambio NISAR, OPERA ve agua nueva en "
          f"{stats['of_nisar_changed_is_opera_new_water']:.1%} "
          f"(agua en t2, nueva o no: {stats['of_nisar_changed_is_opera_water_at_t2']:.1%})")
    print(f"Del agua nueva de OPERA, NISAR marco cambio en "
          f"{stats['of_opera_new_water_is_nisar_changed']:.1%}")
    print(f"Donde NISAR NO ve cambio, OPERA ve agua nueva en "
          f"{stats['of_nisar_unchanged_is_opera_new_water']:.1%}")
    print(f"Celdas con cambio NISAR: {int(with_change.sum())}; corroboradas "
          f"(>= {threshold:.0%}): {corroborated}  -> {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
