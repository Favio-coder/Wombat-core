"""06_cells.py - agregados por celda, PNG del mapa y ensamblado final (§6.8, §6.9, §7).

Entrada:
  --in      delta_db.tif, t1.tif, t2.tif, manifest.json (salidas de 02 y 03)
  --inputs  data-pipeline/inputs/<aoi>/: grid.geojson (de 00) y los insumos de Jelibeth
            (roads.geojson, opera_cells.json, road_cells.json)
Salida (en --in): cells.geojson, manifest.json completo, overlays/t1.png t2.png delta.png bounds.json
Con --bundle <dir>: ademas arma la carpeta del bundle (p. ej. data/piura-01/v1).

Uso:  python data-pipeline/validacion/06_cells.py --in data-pipeline/out/validacion/piura-01 [--bundle data/piura-01/v1]
"""
import argparse
import json
import subprocess
import sys
from pathlib import Path

import numpy as np
from rasterio.warp import Resampling, reproject

from wombat_nisar import aggregate, assemble, contract, overlays, raster_io
from wombat_nisar.grid import Grid

REPO = Path(__file__).resolve().parents[2]


def _commit() -> str:
    r = subprocess.run(["git", "rev-parse", "--short", "HEAD"], cwd=REPO,
                       capture_output=True, text=True)
    return r.stdout.strip() or "unknown"


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--in", dest="in_dir", required=True)
    ap.add_argument("--inputs", help="por defecto data-pipeline/inputs/<aoi_id del manifest>")
    ap.add_argument("--config", default=str(REPO / "config" / "risk-config.json"))
    ap.add_argument("--osm-retrieved-at", help="fecha de descarga de OSM si no viene en roads.geojson")
    ap.add_argument("--bundle", help="carpeta del bundle a crear, p. ej. data/piura-01/v1")
    ap.add_argument("--exploratory", action="store_true",
                    help="procesar aunque el par no sea comparable; nunca arma bundle")
    args = ap.parse_args(argv)
    d = Path(args.in_dir)

    manifest = json.loads((d / "manifest.json").read_text(encoding="utf-8"))
    if manifest.get("nisar", {}).get("comparable") is not True:
        if not args.exploratory or args.bundle:
            print("ERROR: manifest.nisar.comparable != true. No se procesa (R6).")
            return 2
        print("EXPLORATORIO: par NO comparable (R6/R7). Resultado solo para analisis, sin bundle.")
    config = json.loads(Path(args.config).read_text(encoding="utf-8"))
    inputs_dir = Path(args.inputs) if args.inputs else REPO / "data-pipeline" / "inputs" / manifest["aoi_id"]
    if not (inputs_dir / "grid.geojson").exists():
        print(f"ERROR: falta {inputs_dir / 'grid.geojson'}. Correr primero 00_grid.py.")
        return 2
    grid = Grid.from_geojson(inputs_dir / "grid.geojson")
    if grid.aoi_id != manifest["aoi_id"]:
        print(f"ERROR: la grilla es de '{grid.aoi_id}' y el manifest de '{manifest['aoi_id']}'.")
        return 2

    delta, transform, crs = raster_io.read(d / "delta_db.tif")
    raster_io.require_compute_crs(crs, "delta_db.tif", grid.crs)
    t1, t2 = raster_io.read(d / "t1.tif"), raster_io.read(d / "t2.tif")
    raster_io.require_same_grid(t1, t2, "t2.tif")
    # delta_db puede venir en la malla de analisis (bloques de 100 m). Para el mapa se lleva a
    # la malla nativa de t1/t2 sin interpolar, asi las tres capas comparten bounds.json.
    delta_map = np.full(t1[0].shape, np.nan, dtype=np.float32)
    reproject(delta, delta_map, src_transform=transform, src_crs=crs, dst_transform=t1[1],
              dst_crs=t1[2], src_nodata=np.nan, dst_nodata=np.nan, resampling=Resampling.nearest)

    metrics = aggregate.nisar_metrics(delta, transform, grid, config["delta_db_threshold"])
    inputs = assemble.Inputs(inputs_dir, grid)
    cells, warnings = assemble.build_cells(grid, metrics, inputs)
    simulated = manifest.get("data_origin") == "simulated"
    stretch = overlays.write_overlays(d / "overlays", t1[0], t2[0], delta_map, t1[1], t1[2],
                                      simulated=simulated)
    manifest = assemble.final_manifest(manifest, inputs, args.osm_retrieved_at)
    if manifest["pipeline"].get("commit") in (None, "", "unknown"):
        manifest["pipeline"]["commit"] = _commit()

    assemble.dump_json(d / "cells.geojson", cells)
    assemble.dump_json(d / "manifest.json", manifest)

    props = [f["properties"] for f in cells["features"]]
    low = sum(p["valid_fraction"] < config["quality_min_valid_fraction"] for p in props)
    print(f"Celdas: {len(props)}  (calidad insuficiente: {low})  |  cambio medido a {transform.a:g} m")
    print(f"Estiramiento comun t1/t2: {stretch['t1_t2_min_db']} a {stretch['t1_t2_max_db']} dB")
    top = sorted(props, key=lambda p: -p["changed_fraction"])[:3]
    print("Mayor cambio (solo referencia; el ranking lo calcula Rust, R15):")
    for p in top:
        print(f"  {p['cell_id']}  cambio={p['changed_fraction']:.0%}  "
              f"opera={p['opera_status']}  via_cerca={p['road_m_near_change']} m")
    if simulated:
        print("NOTA: escena SIMULADA (data_origin = simulated). No es dato NISAR.")
    for w in warnings:
        print("AVISO:", w)

    problems = ([f"cells.geojson: {e}" for e in contract.errors("cells", cells)]
                + [f"manifest.json: {e}" for e in contract.errors("manifest", manifest)])
    if inputs.roads_path is None:
        problems.append("falta roads.geojson (insumo de Jelibeth)")
    if problems:
        print(f"NO CUMPLE contracts/v1 ({len(problems)}):")
        for p in problems[:20]:
            print("  -", p)
        return 1
    print("OK: manifest.json y cells.geojson cumplen contracts/v1.")

    if args.bundle:
        bundle = assemble.write_bundle(args.bundle, manifest=manifest, cells=cells,
                                       overlays_dir=d / "overlays", roads_path=inputs.roads_path,
                                       config_path=Path(args.config))
        print(f"Bundle (parte Python) en {bundle}. Falta la firma de Rust: wonb/, index.json "
              "y trusted-issuers.json (R14). verify_bundle.py da OK solo despues de eso.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
