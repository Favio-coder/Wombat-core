"""00_grid.py - define la grilla de celdas de 1 km del AOI y publica grid.geojson.

Salida: data-pipeline/inputs/<aoi>/grid.geojson, solo con cell_id y geometria (EPSG:32717).
Es el insumo del que depende Jelibeth: una vez publicada, NO se cambia (cambian los cell_id).

Uso:  python data-pipeline/validacion/00_grid.py --aoi piura-01 --bbox -80.80 -5.35 -80.55 -5.10
"""
import argparse
import json
import sys
from pathlib import Path

from wombat_nisar.grid import Grid

REPO = Path(__file__).resolve().parents[2]


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--aoi", required=True, help="aoi_id, p. ej. piura-01")
    ap.add_argument("--bbox", nargs=4, type=float, required=True,
                    metavar=("OESTE", "SUR", "ESTE", "NORTE"), help="en grados (EPSG:4326)")
    ap.add_argument("--crs", default="EPSG:32717",
                    help="zona UTM del AOI; fuera de EPSG:32717 solo para validar el metodo")
    ap.add_argument("--out", help="por defecto data-pipeline/inputs/<aoi>/grid.geojson")
    ap.add_argument("--force", action="store_true", help="reemplazar una grilla ya publicada")
    args = ap.parse_args(argv)

    out = Path(args.out) if args.out else REPO / "data-pipeline" / "inputs" / args.aoi / "grid.geojson"
    if out.exists() and not args.force:
        print(f"ERROR: {out} ya existe. Cambiar la grilla cambia los cell_id que usa Jelibeth; "
              "si de verdad hay que rehacerla, avisar al grupo y usar --force.")
        return 1
    grid = Grid.from_bbox(args.aoi, args.bbox, args.crs)
    if args.crs != "EPSG:32717":
        print(f"NOTA: grilla en {args.crs}. El contrato v1 fija EPSG:32717, asi que este AOI "
              "sirve para validar el metodo, no para publicar un bundle.")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(grid.to_geojson()) + "\n", encoding="utf-8")
    Grid.from_geojson(out)  # relectura: confirma que el archivo publicado es consistente
    print(f"{out}")
    print(f"  {grid.nrows} filas x {grid.ncols} columnas = {grid.n} celdas de 1 km")
    print(f"  esquina NO ({grid.crs}): x={grid.x0}  y={grid.y_top}")
    print(f"  bbox cubierto (EPSG:4326): {grid.bbox_wgs84()}")
    print(f"  ids: {grid.cell_id(0)} ... {grid.cell_id(grid.n - 1)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
