"""03_change.py - cambio con signo entre t1 y t2 (Plan Maestro §6.4 y §6.5).

Entrada (en --in): t1.tif, t2.tif (gamma0 lineal), mask_t1/t2.tif, looks_t1/t2.tif, manifest.json
Salida  (en --in): delta_db.tif (float32, NaN = invalido), valid.tif, change_summary.json y
                   changed.tif (1 = cambio, 0 = sin cambio, 255 = invalido): la mascara de
                   cambio que Jelibeth usa para vias y OPERA.
Antes de comparar, gamma0 se promedia en bloques (change-config.json, 100 m por defecto) para
que el umbral no mida ruido de speckle. Las salidas quedan en esa malla de analisis.

Uso:  python data-pipeline/validacion/03_change.py --in data-pipeline/out/validacion/piura-01
"""
import argparse
import json
import sys
from pathlib import Path

import numpy as np
from rasterio.warp import Resampling, reproject

from wombat_nisar import change, raster_io
from wombat_nisar.gcov import MASK_RULE, load_mask_rule

REPO = Path(__file__).resolve().parents[2]


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--in", dest="in_dir", required=True)
    ap.add_argument("--config", default=str(REPO / "config" / "risk-config.json"))
    ap.add_argument("--mask-rule", default=str(MASK_RULE))
    ap.add_argument("--change-config", default=str(MASK_RULE.parent / "change-config.json"))
    ap.add_argument("--exploratory", action="store_true",
                    help="procesar aunque el par no sea comparable; no sirve para entregar")
    ap.add_argument("--exclude", help="mascara de exclusion (make_exclude.py); por defecto "
                                      "<in>/exclude.tif si existe")
    ap.add_argument("--multilook-m", type=float,
                    help="tamano del bloque de promediado en metros; 0 = pixel a pixel. "
                         "Por defecto, el de change-config.json")
    args = ap.parse_args(argv)
    d = Path(args.in_dir)

    manifest = json.loads((d / "manifest.json").read_text(encoding="utf-8"))
    if manifest.get("nisar", {}).get("comparable") is not True:
        if not args.exploratory:
            print("ERROR: manifest.nisar.comparable != true. No se procesa (R6).")
            return 2
        print("EXPLORATORIO: par NO comparable (R6/R7). Resultado solo para analisis.")
    threshold = json.loads(Path(args.config).read_text(encoding="utf-8"))["delta_db_threshold"]
    rule = load_mask_rule(args.mask_rule)
    if not rule.get("confirmed"):
        print("AVISO: los valores validos de la mascara NO estan confirmados con la "
              "especificacion (R11). Ver data-pipeline/validacion/gcov-mask.json.")

    t1 = raster_io.read(d / "t1.tif")
    raster_io.require_compute_crs(t1[2], "t1.tif", manifest.get("crs_compute", raster_io.CRS_COMPUTE))
    layers = {}
    for name in ("t2", "mask_t1", "mask_t2", "looks_t1", "looks_t2"):
        layers[name] = raster_io.read(d / f"{name}.tif")
        raster_io.require_same_grid(t1, layers[name], f"{name}.tif")

    valid = change.quality_gate(t1[0], layers["t2"][0], layers["mask_t1"][0],
                                layers["mask_t2"][0], layers["looks_t1"][0],
                                layers["looks_t2"][0], rule["valid_values"])
    cfg = json.loads(Path(args.change_config).read_text(encoding="utf-8"))
    size_m = cfg["multilook_meters"] if args.multilook_m is None else args.multilook_m
    native_m = t1[1].a
    if size_m and size_m > native_m:
        g1, g2, valid_a, transform, k = change.multilook(
            t1[0], layers["t2"][0], valid, t1[1], size_m, cfg["min_valid_fraction_per_block"])
    else:
        g1, g2, valid_a, transform, k, size_m = t1[0], layers["t2"][0], valid, t1[1], 1, 0
    exclude_path = Path(args.exclude) if args.exclude else d / "exclude.tif"
    excluded = None
    if exclude_path.exists():  # R13: agua permanente y cambio habitual pasan a dato invalido
        ex, ex_transform, ex_crs = raster_io.read(exclude_path)
        on_grid = np.full(valid_a.shape, 255, dtype=np.uint8)
        reproject(ex, on_grid, src_transform=ex_transform, src_crs=ex_crs, dst_transform=transform,
                  dst_crs=t1[2], dst_nodata=255, resampling=Resampling.nearest)
        excluded = float(((on_grid == 1) & valid_a).mean())
        uncovered = float((on_grid == 255).mean())
        valid_a = valid_a & (on_grid != 1)
        print(f"Mascara de exclusion {exclude_path.name}: {excluded:.1%} del area excluida")
        if uncovered > 0.01:
            print(f"AVISO: la mascara no cubre {uncovered:.0%} del AOI; ahi no se excluye nada.")
    elif args.exclude:
        print(f"ERROR: no existe {exclude_path}.")
        return 2
    delta = change.delta_db(g1, g2, valid_a)
    raster_io.write(d / "delta_db.tif", delta, transform, t1[2], nodata=np.nan)
    raster_io.write(d / "valid.tif", valid_a.astype(np.uint8), transform, t1[2])
    raster_io.write(d / "changed.tif", change.changed_mask(delta, threshold), transform, t1[2],
                    nodata=change.CHANGED_NODATA)

    s = change.summary(delta, threshold)
    s["delta_db_threshold"] = threshold
    s["mask_rule_confirmed"] = bool(rule.get("confirmed"))
    s["native_pixel_m"] = native_m
    s["multilook_meters"] = size_m
    s["analysis_pixel_m"] = transform.a
    s["excluded_fraction"] = None if excluded is None else round(excluded, 4)
    if valid.any():
        looks = float(np.median(np.minimum(layers["looks_t1"][0], layers["looks_t2"][0])[valid]))
        noise = change.speckle_std_db(looks * k * k)
        false = change.false_change_fraction(threshold, noise)
        s.update(native_looks_median=round(looks, 2), effective_looks=round(looks * k * k, 1),
                 speckle_std_db=round(noise, 3), expected_false_change_fraction=round(false, 5))
        if false > 0.01:
            s["warnings"].append(
                f"umbral dominado por ruido: ~{false:.0%} de pixeles sin cambio real cruzaria "
                f"+-{threshold} dB solo por speckle; subir multilook_meters")
    (d / "change_summary.json").write_text(json.dumps(s, indent=2), encoding="utf-8")

    print(f"Analisis a {transform.a:g} m (pixel nativo {native_m:g} m, "
          f"{'promediado en bloques' if k > 1 else 'sin promediar'})")
    if "speckle_std_db" in s:
        print(f"Ruido de speckle esperado: {s['speckle_std_db']:.2f} dB "
              f"({s['effective_looks']:g} looks); falso cambio esperado: "
              f"{s['expected_false_change_fraction']:.2%}")
    print(f"Pixeles validos: {s['valid_fraction']:.1%}")
    if "p50_db" in s:
        print(f"delta_db  p02={s['p02_db']:+.2f}  p50={s['p50_db']:+.2f}  p98={s['p98_db']:+.2f}"
              f"  desv={s['std_db']:.2f} dB")
        print(f"Bajo >= {threshold} dB: {s['decrease_fraction']:.1%}   "
              f"Subio >= {threshold} dB: {s['increase_fraction']:.1%}")
    for w in s["warnings"]:
        print("AVISO:", w)
    return 1 if any(w.startswith("TODO") for w in s["warnings"]) else 0


if __name__ == "__main__":
    sys.exit(main())
