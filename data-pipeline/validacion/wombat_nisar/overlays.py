"""PNG para el mapa y bounds.json (Plan Maestro §6.9, R9, R10)."""
from __future__ import annotations

import json
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw
from rasterio.transform import array_bounds
from rasterio.warp import Resampling, calculate_default_transform, reproject

DELTA_RANGE_DB = 6.0  # delta.png fijo en +-6 dB
_BLUE = np.array([33, 102, 172], dtype=np.float32)   # bajo
_RED = np.array([178, 24, 43], dtype=np.float32)     # subio
_WHITE = np.array([247, 247, 247], dtype=np.float32)


def to_db(gamma) -> np.ndarray:
    out = np.full(gamma.shape, np.nan, dtype=np.float32)
    with np.errstate(invalid="ignore"):
        ok = np.isfinite(gamma) & (gamma > 0)
    out[ok] = 10.0 * np.log10(gamma[ok])
    return out


def shared_stretch(db1, db2, valid) -> tuple[float, float]:
    """Un solo (min, max) para t1 y t2: percentiles 2-98 sobre t1 U t2 (R9)."""
    values = np.concatenate([db1[valid], db2[valid]])
    values = values[np.isfinite(values)]
    if values.size == 0:
        raise ValueError("No hay pixeles validos para calcular el estiramiento")
    vmin, vmax = np.percentile(values, [2, 98])
    return float(vmin), float(vmax)


def _to_wgs84(arr, transform, crs, grid):
    dst_transform, width, height = grid
    dst = np.full((height, width), np.nan, dtype=np.float32)
    reproject(arr.astype(np.float32), dst, src_transform=transform, src_crs=crs,
              dst_transform=dst_transform, dst_crs="EPSG:4326",
              src_nodata=np.nan, dst_nodata=np.nan, resampling=Resampling.nearest)
    return dst


def _gray_rgba(db, vmin, vmax) -> np.ndarray:
    ok = np.isfinite(db)
    scaled = np.clip((np.nan_to_num(db, nan=vmin) - vmin) / (vmax - vmin), 0, 1)
    level = (scaled * 255).astype(np.uint8)
    return np.dstack([level, level, level, np.where(ok, 255, 0).astype(np.uint8)])


def _diverging_rgba(delta) -> np.ndarray:
    ok = np.isfinite(delta)
    t = np.clip(np.nan_to_num(delta, nan=0.0) / DELTA_RANGE_DB, -1, 1)[..., None]
    rgb = np.where(t < 0, _WHITE + (-t) * (_BLUE - _WHITE), _WHITE + t * (_RED - _WHITE))
    return np.dstack([rgb.astype(np.uint8), np.where(ok, 255, 0).astype(np.uint8)])


def _save(rgba, path, simulated: bool) -> None:
    image = Image.fromarray(rgba)
    if simulated:  # R1: el dato sintetico se rotula en la propia imagen
        draw = ImageDraw.Draw(image)
        for y in range(10, image.height, 90):
            draw.text((10, y), "SIMULATED DATA", fill=(255, 200, 0, 255))
    image.save(path)


def write_overlays(out_dir: str | Path, g1, g2, delta, transform, crs,
                   simulated: bool = False) -> dict:
    """Escribe t1.png, t2.png, delta.png y bounds.json. Devuelve el estiramiento usado."""
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    height, width = delta.shape
    grid = calculate_default_transform(crs, "EPSG:4326", width, height,
                                       *array_bounds(height, width, transform))
    db1, db2 = to_db(g1), to_db(g2)
    vmin, vmax = shared_stretch(db1, db2, np.isfinite(delta))

    _save(_gray_rgba(_to_wgs84(db1, transform, crs, grid), vmin, vmax), out_dir / "t1.png", simulated)
    _save(_gray_rgba(_to_wgs84(db2, transform, crs, grid), vmin, vmax), out_dir / "t2.png", simulated)
    _save(_diverging_rgba(_to_wgs84(delta, transform, crs, grid)), out_dir / "delta.png", simulated)

    dst_transform, w, h = grid
    west, north = dst_transform.c, dst_transform.f
    east, south = west + w * dst_transform.a, north + h * dst_transform.e
    corners = [[west, north], [east, north], [east, south], [west, south]]
    bounds = {  # misma forma que contracts/v1/fixtures/bundle/piura-demo/v0/overlays/bounds.json
        "crs": "EPSG:4326",
        "coordinates": [[round(lon, 6), round(lat, 6)] for lon, lat in corners],
        "order": "top-left, top-right, bottom-right, bottom-left",
    }
    (out_dir / "bounds.json").write_text(json.dumps(bounds, indent=2) + "\n", encoding="utf-8")
    return {"t1_t2_min_db": round(vmin, 2), "t1_t2_max_db": round(vmax, 2),
            "delta_min_db": -DELTA_RANGE_DB, "delta_max_db": DELTA_RANGE_DB}
