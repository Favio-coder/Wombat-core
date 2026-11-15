"""02_subset.py - recorta t1 y t2 al AOI y escribe el manifest (Plan Maestro §6.2).

Entrada: dos granules GCOV. Cada uno puede ser
  - un archivo .h5 local, o
  - el nombre de un granule que este en candidates.json (se lee por partes desde Earthdata,
    sin bajar el HDF5 completo; necesita el login: python data-pipeline/validacion/earthdata_login.py).
Salida (en --out): t1.tif, t2.tif (gamma0 lineal), mask_t1/t2.tif, looks_t1/t2.tif, manifest.json

Uso:
  python data-pipeline/validacion/02_subset.py --aoi piura-01 --aoi-name "Bajo Piura" \
      --t1 NISAR_L2_PR_GCOV_..._001 --t2 NISAR_L2_PR_GCOV_..._001
Si un chequeo de comparabilidad falla, escribe el manifest con comparable=false y NO recorta (R6).
"""
import argparse
import json
import subprocess
import sys
from pathlib import Path

import numpy as np

from wombat_nisar import raster_io, subset
from wombat_nisar.grid import Grid
from wombat_nisar.manifest import build_manifest

REPO = Path(__file__).resolve().parents[2]


def _resolve(value: str, candidates: dict, maturity: str | None):
    """Devuelve (fuente para abrir, nombre del granule, madurez)."""
    by_name = {c["granule"]: c for c in candidates.get("candidates", [])}
    local = Path(value)
    name = local.stem if local.suffix in (".h5", ".hdf5") else value
    known = by_name.get(name)
    level = maturity or (known or {}).get("maturity")
    if not level:
        raise SystemExit(f"No se la madurez de {name}: pasar --maturity o un candidates.json que lo liste.")
    if local.exists():
        return local, name, level
    if known and known.get("url"):
        return known["url"], name, level
    raise SystemExit(f"{value}: no es un archivo local ni un granule de candidates.json.")


def _commit() -> str:
    r = subprocess.run(["git", "rev-parse", "--short", "HEAD"], cwd=REPO,
                       capture_output=True, text=True)
    return r.stdout.strip() or "unknown"


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--aoi", required=True)
    ap.add_argument("--aoi-name", required=True)
    ap.add_argument("--t1", required=True)
    ap.add_argument("--t2", required=True)
    ap.add_argument("--grid", help="por defecto data-pipeline/inputs/<aoi>/grid.geojson")
    ap.add_argument("--candidates", help="por defecto <out>/candidates.json")
    ap.add_argument("--maturity", choices=("PROVISIONAL", "VALIDATED", "BETA"),
                    help="solo si los granules no estan en candidates.json")
    ap.add_argument("--frequency", default="A", choices=("A", "B"))
    ap.add_argument("--polarization", default="HHHH")
    ap.add_argument("--out", help="por defecto data-pipeline/out/validacion/<aoi>")
    ap.add_argument("--exploratory", action="store_true",
                    help="recortar aunque el par no sea comparable; no sirve para entregar")
    args = ap.parse_args(argv)

    # Salida propia, separada de data-pipeline/out/validacion/<aoi>/ que usa el pipeline oficial.
    out = Path(args.out) if args.out else REPO / "data-pipeline" / "out" / "validacion" / args.aoi
    grid_path = Path(args.grid) if args.grid else REPO / "data-pipeline" / "inputs" / args.aoi / "grid.geojson"
    if not grid_path.exists():
        print(f"ERROR: falta {grid_path}. Correr primero 00_grid.py.")
        return 2
    grid = Grid.from_geojson(grid_path)
    if grid.aoi_id != args.aoi:
        print(f"ERROR: la grilla es de '{grid.aoi_id}', no de '{args.aoi}'.")
        return 2
    cand_path = Path(args.candidates) if args.candidates else out / "candidates.json"
    candidates = json.loads(cand_path.read_text(encoding="utf-8")) if cand_path.exists() else {}
    bounds = (grid.x0, grid.y_top - grid.nrows * 1000, grid.x0 + grid.ncols * 1000, grid.y_top)

    scenes, sources = {}, {}
    for key in ("t1", "t2"):
        source, name, maturity = _resolve(getattr(args, key), candidates, args.maturity)
        print(f"{key}: {name}  ({'local' if isinstance(source, Path) else 'por partes desde Earthdata'})")
        with subset.open_granule(source) as h5:
            info = subset.describe(h5, args.frequency, args.polarization)
        scene = subset.check_against_name(info, name)
        scene.update(maturity=maturity, frequency=args.frequency,
                     polarization=args.polarization, info=info)
        scenes[key], sources[key] = scene, source

    info = scenes["t1"]["info"]
    manifest = build_manifest(
        aoi_id=grid.aoi_id, aoi_name=args.aoi_name, bbox_wgs84=grid.bbox_wgs84(),
        t1=scenes["t1"], t2=scenes["t2"], pixel_spacing_m=round(info["pixel_spacing_m"]),
        pipeline_commit=_commit(), crs_compute=grid.crs)
    if grid.crs != "EPSG:32717":
        print(f"NOTA: AOI en {grid.crs}, no en la zona de piura-01 (EPSG:32717). "
              "Valido desde el contrato v1.1.0.")
    out.mkdir(parents=True, exist_ok=True)
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n",
                                       encoding="utf-8")
    checks = manifest["nisar"]["comparable_checks"]
    for name, ok in checks.items():
        print(f"  {'ok ' if ok else 'NO '} {name}")
    if not manifest["nisar"]["comparable"]:
        if not args.exploratory:
            print("NO COMPARABLE: se escribio manifest.json con comparable=false y no se recorta (R6).")
            return 2
        print("EXPLORATORIO: el par NO es comparable (R6/R7). Se recorta solo para analisis; "
              "el manifest queda con comparable=false y Rust lo rechazara.")
    for key in ("t1", "t2"):
        if f"EPSG:{scenes[key]['info']['epsg']}" != grid.crs:
            print(f"ERROR: {key} esta en EPSG:{scenes[key]['info']['epsg']} y la grilla en "
                  f"{grid.crs} (R10). Rehacer la grilla con --crs EPSG:{scenes[key]['info']['epsg']}.")
            return 2

    data = {}
    for key in ("t1", "t2"):
        with subset.open_granule(sources[key]) as h5:  # solo se recorta si el par es comparable
            data[key] = subset.read_window(h5, args.frequency, args.polarization, bounds)
    a, b = data["t1"], data["t2"]
    if a["transform"] != b["transform"] or a["gamma"].shape != b["gamma"].shape:
        print("ERROR: t1 y t2 no comparten malla en el AOI; habria que remuestrear y declararlo.")
        return 2
    for key, d in data.items():
        raster_io.write(out / f"{key}.tif", d["gamma"], d["transform"], d["crs"], nodata=np.nan)
        raster_io.write(out / f"mask_{key}.tif", d["mask"], d["transform"], d["crs"], nodata=255)
        raster_io.write(out / f"looks_{key}.tif", d["looks"], d["transform"], d["crs"], nodata=np.nan)
        finite = np.isfinite(d["gamma"])
        print(f"{key}: {d['gamma'].shape[1]} x {d['gamma'].shape[0]} pixeles, "
              f"{info['pixel_spacing_m']:g} m, cubre {min(d['coverage'], 1):.0%} del AOI, "
              f"{finite.mean():.0%} con dato")
    print(f"OK: comparable={str(manifest['nisar']['comparable']).lower()}. Salidas en {out}. "
          "Sigue 03_change.py.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
