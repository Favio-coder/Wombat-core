"""Lectura y escritura de rasteres de una banda."""
from __future__ import annotations

from pathlib import Path

import numpy as np
import rasterio

CRS_COMPUTE = "EPSG:32717"  # R10: todos los calculos en UTM 17S


def read(path: str | Path):
    """Devuelve (array 2D, transform, crs)."""
    with rasterio.open(path) as src:
        return src.read(1), src.transform, src.crs


def write(path: str | Path, arr: np.ndarray, transform, crs, nodata=None) -> None:
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    profile = dict(driver="GTiff", height=arr.shape[0], width=arr.shape[1], count=1,
                   dtype=arr.dtype, crs=crs, transform=transform, compress="deflate")
    if nodata is not None:
        profile["nodata"] = nodata
    with rasterio.open(path, "w", **profile) as dst:
        dst.write(arr, 1)


def require_compute_crs(crs, name: str, expected: str = CRS_COMPUTE) -> None:
    """`expected` es el crs_compute del manifest; el contrato v1 solo admite EPSG:32717."""
    if crs is None or crs.to_string() != expected:
        raise ValueError(f"{name}: CRS {crs} != {expected} (R10)")


def require_same_grid(ref, other, name: str) -> None:
    """ref y other son tuplas (array, transform, crs)."""
    if ref[0].shape != other[0].shape or ref[1] != other[1] or ref[2] != other[2]:
        raise ValueError(f"{name}: la malla no coincide con t1 (forma, transform o CRS)")
