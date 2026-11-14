"""Reproject PNG pixels to WGS84, with shared radiometry and signed colours."""
from pathlib import Path
import numpy as np
from PIL import Image
from rasterio.warp import calculate_default_transform, reproject, Resampling
from rasterio.transform import array_bounds
from common import CRS, write_json

def render(out, t1, t2, delta, affine):
    out = Path(out) / 'overlays'
    out.mkdir(parents=True, exist_ok=True)
    h, w = delta.shape
    target, width, height = calculate_default_transform(CRS, 'EPSG:4326', w, h, *array_bounds(h, w, affine))
    valid = np.isfinite(delta)
    db1, db2 = [np.where(valid, 10*np.log10(np.maximum(x, 1e-30)), np.nan) for x in (t1, t2)]
    sample = np.concatenate([db1[valid], db2[valid]])
    if not sample.size:
        raise ValueError('No valid data for overlays')
    low, high = np.percentile(sample, [2, 98])
    if high <= low:
        high = low + 1
    for name, data in [('t1', db1), ('t2', db2), ('delta', delta)]:
        dst = np.full((height, width), np.nan, 'float32')
        reproject(data.astype('float32'), dst, src_transform=affine, src_crs=CRS, src_nodata=np.nan,
                  dst_transform=target, dst_crs='EPSG:4326', dst_nodata=np.nan, resampling=Resampling.nearest)
        alpha = np.isfinite(dst)
        rgba = np.zeros((height, width, 4), 'uint8')
        if name == 'delta':
            scale = np.nan_to_num(np.clip(dst / 6, -1, 1), nan=0)
            rgba[..., 0] = np.rint(255*(1+np.minimum(scale, 0))).astype('uint8')
            rgba[..., 1] = np.rint(255*(1-np.abs(scale))).astype('uint8')
            rgba[..., 2] = np.rint(255*(1-np.maximum(scale, 0))).astype('uint8')
        else:
            grey = np.rint(255*np.nan_to_num(np.clip((dst-low)/(high-low), 0, 1), nan=0)).astype('uint8')
            rgba[..., :3] = grey[..., None]
        rgba[..., 3] = alpha.astype('uint8')*255
        Image.fromarray(rgba).save(out / (name+'.png'))
    west, south, east, north = array_bounds(height, width, target)
    write_json(out / 'bounds.json', {'crs': 'EPSG:4326',
        'coordinates': [[west,north],[east,north],[east,south],[west,south]],
        'order': 'top-left, top-right, bottom-right, bottom-left'})
    write_json(out / 'scales.json', {'t1_t2_db': [float(low), float(high)], 'delta_db': [-6,6],
                                   'decrease': 'blue', 'increase': 'red', 'invalid': 'transparent'})
