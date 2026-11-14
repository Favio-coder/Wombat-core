"""Ensamblado final: cells.geojson, manifest.json y carpeta del bundle.

Une los agregados NISAR con los insumos de Jelibeth en data-pipeline/inputs/<aoi>/:
  roads.geojson     red vial OSM. Metadatos en la raiz del archivo (o en "metadata"):
                    "retrieved_at", "query", "attribution".
  opera_cells.json  por cell_id: opera_status, opera_overlap_ratio. Opcional en la raiz:
                    "status", "t1", "t2" (granules y gap_days) para el manifest.
  road_cells.json   por cell_id: road_m_near_change, road_m_total.
Los archivos por celda pueden ser {cell_id: {...}}, una lista de objetos con "cell_id",
o cualquiera de las dos dentro de la clave "cells". Nada se edita a mano.
"""
from __future__ import annotations

import json
import shutil
from pathlib import Path

import geopandas as gpd

from .grid import Grid
from .manifest import OPERA_UNAVAILABLE

OSM_ATTRIBUTION = "© OpenStreetMap contributors"


def _load(path: Path):
    return json.loads(path.read_text(encoding="utf-8")) if path.exists() else None


def _per_cell(doc, name: str, grid: Grid) -> dict:
    cells = doc.get("cells", doc) if isinstance(doc, dict) else doc
    if isinstance(cells, list):
        cells = {entry["cell_id"]: entry for entry in cells}
    cells = {k: v for k, v in cells.items() if isinstance(v, dict)}
    unknown = sorted(set(cells) - set(grid.ids()))
    if unknown:
        raise ValueError(f"{name}: {len(unknown)} cell_id que no estan en grid.geojson, "
                         f"p. ej. {unknown[:3]}")
    return cells


def _timestamp(value: str) -> str:
    return value if "T" in value else f"{value}T00:00:00Z"


class Inputs:
    """Insumos de Jelibeth. Lo que falta queda en None y se informa, no se inventa."""

    def __init__(self, inputs_dir: str | Path, grid: Grid):
        d = Path(inputs_dir)
        self.roads_path = d / "roads.geojson" if (d / "roads.geojson").exists() else None
        opera_doc = _load(d / "opera_cells.json")
        road_doc = _load(d / "road_cells.json")
        self.opera = _per_cell(opera_doc, "opera_cells.json", grid) if opera_doc else None
        self.roads = _per_cell(road_doc, "road_cells.json", grid) if road_doc else None
        self.opera_meta = opera_doc if isinstance(opera_doc, dict) else {}
        roads_doc = _load(self.roads_path) if self.roads_path else {}
        self.osm_meta = {**roads_doc.get("metadata", {}),
                         **{k: roads_doc[k] for k in ("retrieved_at", "query", "attribution")
                            if k in roads_doc}}


def _ratio(opera: dict | None):
    """4 decimales, como el resto de fracciones: Rust lee el decimal escrito y lo vuelve exacto
    (wombat-risk, Exact::from_f64_repr); un 0.3333333333333333 desbordaria su aritmetica."""
    value = opera.get("opera_overlap_ratio") if opera else None
    return None if value is None else round(float(value), 4)


def _meters(roads: dict | None, key: str) -> int:
    """Metros enteros (Rust los lee como u32)."""
    return int(round(float(roads[key]))) if roads else 0


def build_cells(grid: Grid, metrics: dict, inputs: Inputs) -> tuple[dict, list[str]]:
    """FeatureCollection de cells.geojson (geometria en EPSG:4326) y avisos."""
    warnings, feats = [], []
    no_opera = no_roads = 0
    for k in range(grid.n):
        cid = grid.cell_id(k)
        opera = (inputs.opera or {}).get(cid)
        roads = (inputs.roads or {}).get(cid)
        no_opera += opera is None
        no_roads += roads is None
        feats.append({
            "type": "Feature",
            "geometry": {"type": "Polygon", "coordinates": [grid.ring_wgs84(k)]},
            "properties": {
                "cell_id": cid,
                **metrics[cid],
                "opera_status": opera["opera_status"] if opera else "unavailable",
                "opera_overlap_ratio": _ratio(opera),
                "road_m_near_change": _meters(roads, "road_m_near_change"),
                "road_m_total": _meters(roads, "road_m_total"),
            },
        })
    if inputs.opera is None:
        warnings.append("sin opera_cells.json: OPERA queda 'unavailable' en todas las celdas")
    elif no_opera:
        warnings.append(f"{no_opera} celdas sin fila en opera_cells.json: quedan 'unavailable'")
    if inputs.roads is None:
        warnings.append("sin road_cells.json: road_m_* queda en 0 (NO es el dato final)")
    elif no_roads:
        warnings.append(f"{no_roads} celdas sin fila en road_cells.json: quedan en 0")
    fc = {"type": "FeatureCollection",
          "crs_note": "geometry en EPSG:4326 para el mapa; calculos hechos en EPSG:32717",
          "features": feats}
    return fc, warnings


def final_manifest(manifest: dict, inputs: Inputs, osm_retrieved_at: str | None = None) -> dict:
    """Completa `opera` y `osm`. Si falta la fecha de OSM, `osm` no se escribe y el esquema falla."""
    out = dict(manifest)
    meta = inputs.opera_meta
    if inputs.opera and any(c.get("opera_status") != "unavailable" for c in inputs.opera.values()):
        out["opera"] = {
            "product": "OPERA_L3_DSWx-S1", "derived_from": "Sentinel-1", "status": "available",
            "t1": meta.get("t1", {"granules": [], "gap_days": None}),
            "t2": meta.get("t2", {"granules": [], "gap_days": None}),
        }
    else:
        out["opera"] = dict(OPERA_UNAVAILABLE)
    retrieved_at = osm_retrieved_at or inputs.osm_meta.get("retrieved_at")
    if retrieved_at:
        out["osm"] = {"query": inputs.osm_meta.get("query", "highway=*"),
                      "retrieved_at": _timestamp(retrieved_at), "attribution": OSM_ATTRIBUTION}
    else:
        out.pop("osm", None)
    return out


def dump_json(path: Path, doc: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(doc, indent=2, ensure_ascii=False, allow_nan=False) + "\n",
                    encoding="utf-8")


def write_bundle(bundle_dir: str | Path, *, manifest: dict, cells: dict, overlays_dir: Path,
                 roads_path: Path, config_path: Path) -> Path:
    """Arma data/<aoi>/<vN>/ con lo que produce Python. Los bloques wonb/, index.json y
    trusted-issuers.json los agrega Rust al firmar (R14). Nunca sobrescribe una version (R28)."""
    bundle = Path(bundle_dir)
    if bundle.exists() and any(bundle.iterdir()):
        raise FileExistsError(f"{bundle} ya existe: se publica una version nueva (R28)")
    dump_json(bundle / "manifest.json", manifest)
    dump_json(bundle / "cells.geojson", cells)
    (bundle / "overlays").mkdir(parents=True, exist_ok=True)
    for name in ("t1.png", "t2.png", "delta.png", "bounds.json"):
        shutil.copyfile(overlays_dir / name, bundle / "overlays" / name)
    shutil.copyfile(config_path, bundle / "risk-config.json")

    roads = gpd.read_file(roads_path)                      # se simplifica y pasa a EPSG:4326
    keep = [c for c in ("highway", "name") if c in roads.columns]
    roads = roads[keep + ["geometry"]].copy()
    roads["geometry"] = roads.geometry.simplify(5)
    roads = roads.to_crs("EPSG:4326")
    if manifest.get("data_origin") == "simulated":
        roads["simulated"] = True
    (bundle / "roads.geojson").write_text(roads.to_json(drop_id=True) + "\n", encoding="utf-8")
    return bundle
