"""Inspecciona un granule GCOV local (equivalente a `h5ls -r`, que no viene en Windows).

Sirve para confirmar en el dato REAL, antes de codificar nada (R11):
  - las rutas de HHHH, mask, numberOfLooks, coordenadas y proyeccion;
  - los atributos de la mascara (descripcion, valores validos, relleno);
  - los valores que la mascara toma de verdad.

Uso:  python data-pipeline/validacion/inspect_granule.py ruta/al/granule.h5 [--frequency A]
"""
import argparse

import h5py
import numpy as np

from wombat_nisar.gcov import GRID_GROUP


def _show_attrs(obj, indent="      "):
    for key, value in obj.attrs.items():
        if isinstance(value, bytes):
            value = value.decode("utf-8", "replace")
        print(f"{indent}@{key} = {value}")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("granule")
    ap.add_argument("--frequency", default="A")
    args = ap.parse_args()

    with h5py.File(args.granule, "r") as h5:
        ident = "/science/LSAR/identification"
        if ident in h5:
            print(f"== {ident}")
            for name, ds in h5[ident].items():
                if isinstance(ds, h5py.Dataset) and ds.size <= 8:
                    value = ds[()]
                    if isinstance(value, bytes):
                        value = value.decode("utf-8", "replace")
                    print(f"   {name} = {value}")

        group = GRID_GROUP.format(frequency=args.frequency)
        if group not in h5:
            print(f"NO EXISTE {group}. Grupos en grids:",
                  list(h5.get("/science/LSAR/GCOV/grids", {}).keys()))
            return
        print(f"\n== {group}")
        for name, ds in h5[group].items():
            if isinstance(ds, h5py.Dataset):
                print(f"   {name}  shape={ds.shape}  dtype={ds.dtype}")
                _show_attrs(ds)

        if "mask" in h5[group]:
            mask = h5[group]["mask"]
            step = max(1, min(mask.shape) // 400)
            values, counts = np.unique(mask[::step, ::step], return_counts=True)
            print("\n== Valores de la mascara (muestra 1 de cada", step, "pixeles)")
            for v, c in zip(values, counts):
                print(f"   {v}: {c / counts.sum():.1%}")
            print("   -> comparar con la especificacion y actualizar gcov-mask.json")


if __name__ == "__main__":
    main()
