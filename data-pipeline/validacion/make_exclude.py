"""make_exclude.py - arma la mascara de exclusion (R13) a partir de un par BASE.

Par base = dos escenas sin evento entre ellas, ya procesadas con 02_subset.py y 03_change.py.
Excluye agua permanente (oscura en las dos fechas) y zonas que ya cambian sin evento
(cultivos, riego, mareas). 03_change.py la aplica si encuentra exclude.tif en su carpeta
o si se le pasa --exclude.

Uso:
  python data-pipeline/validacion/make_exclude.py --base data-pipeline/out/validacion/tumbes-01/base-pista003 \
      --out data-pipeline/out/validacion/tumbes-01/exclude.tif
Salida: exclude.tif (1 = excluir, 0 = usar; malla de analisis, EPSG:32717) y exclude.json
con los parametros y las escenas usadas, para declararlo en la pantalla Metodo.
"""
import argparse
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

import numpy as np

from wombat_nisar import exclude, raster_io

NISAR = Path(__file__).resolve().parent
REPO = NISAR.parents[1]


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--base", required=True, help="carpeta del par base (salidas de 02 y 03)")
    ap.add_argument("--out", required=True, help="ruta de exclude.tif")
    ap.add_argument("--water-db", type=float, default=-15.0,
                    help="gamma0 (dB) bajo el cual un bloque oscuro en ambas fechas es agua")
    ap.add_argument("--change-db", type=float,
                    help="por defecto, delta_db_threshold de risk-config.json")
    ap.add_argument("--grow-blocks", type=int, default=1, help="bloques de margen alrededor")
    ap.add_argument("--config", default=str(REPO / "config" / "risk-config.json"))
    args = ap.parse_args(argv)
    base = Path(args.base)

    summary = json.loads((base / "change_summary.json").read_text(encoding="utf-8"))
    if not summary.get("multilook_meters"):
        print("ERROR: el par base debe procesarse con promediado (multilook_meters > 0).")
        return 2
    change_db = args.change_db
    if change_db is None:
        change_db = json.loads(Path(args.config).read_text(encoding="utf-8"))["delta_db_threshold"]
    t1, t2 = raster_io.read(base / "t1.tif"), raster_io.read(base / "t2.tif")
    delta, _, crs = raster_io.read(base / "delta_db.tif")
    valid = np.isfinite(t1[0]) & np.isfinite(t2[0]) & (t1[0] > 0) & (t2[0] > 0)
    mask, water, habitual, transform = exclude.build(
        t1[0], t2[0], valid, t1[1], delta, size_m=summary["multilook_meters"],
        water_db=args.water_db, change_db=change_db, grow_blocks=args.grow_blocks)

    out = Path(args.out)
    raster_io.write(out, mask.astype(np.uint8), transform, crs)
    nisar = json.loads((base / "manifest.json").read_text(encoding="utf-8"))["nisar"]
    info = {
        "created_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "base_pair": {"t1": nisar["t1"], "t2": nisar["t2"], "maturity": nisar["maturity"]},
        "block_m": summary["multilook_meters"],
        "water_db": args.water_db, "change_db": change_db, "grow_blocks": args.grow_blocks,
        "fraction_water": round(float(water.mean()), 4),
        "fraction_habitual_change": round(float(habitual.mean()), 4),
        "fraction_excluded": round(float(mask.mean()), 4),
        "rule": "R13: agua permanente y zonas que cambian sin evento se enmascaran y se declara",
    }
    out.with_suffix(".json").write_text(json.dumps(info, indent=2), encoding="utf-8")
    print(f"{out}")
    print(f"  agua permanente (< {args.water_db} dB en ambas fechas): {info['fraction_water']:.1%}")
    print(f"  cambio habitual (>= {change_db} dB sin evento): {info['fraction_habitual_change']:.1%}")
    print(f"  excluido en total, con margen: {info['fraction_excluded']:.1%}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
