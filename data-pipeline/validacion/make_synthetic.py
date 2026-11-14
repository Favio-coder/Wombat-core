"""Genera una escena SINTETICA (no es dato NISAR) para probar 03 y 06.

Uso:  python data-pipeline/validacion/make_synthetic.py --out data-pipeline/out/validacion/synth-01 \
          --inputs data-pipeline/out/validacion/synth-01/inputs
"""
import argparse

from wombat_nisar.synthetic import make_scene

if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", required=True)
    ap.add_argument("--inputs", required=True)
    args = ap.parse_args()
    grid = make_scene(args.out, args.inputs)
    print(f"Escena sintetica en {args.out} ({grid.n} celdas). SIMULATED DATA.")
