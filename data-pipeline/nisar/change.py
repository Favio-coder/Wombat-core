"""Signed gamma-zero change; fail closed on incomparable inputs."""
import argparse
from pathlib import Path
import numpy as np
import rasterio
from common import CRS, OUT, read_json
from search import checks
from subset import write_tif

def compute_change(t1, t2, mask1, mask2, looks1, looks2, swaths1, swaths2, comparable):
    if not comparable:
        raise ValueError('R6: scenes are not comparable')
    arrays = (t1, t2, mask1, mask2, looks1, looks2)
    if len({x.shape for x in arrays}) != 1:
        raise ValueError('All raster dimensions must agree')
    # Rev F mask: 0 invalid/partly focused; 255 outside acquisition;
    # 1..numberOfSubSwaths are valid subswath IDs, NOT layover bit flags.
    valid = ((mask1 >= 1) & (mask1 <= swaths1) & (mask2 >= 1) & (mask2 <= swaths2)
             & np.isfinite(t1) & np.isfinite(t2) & (t1 > 0) & (t2 > 0)
             & np.isfinite(looks1) & np.isfinite(looks2) & (looks1 > 0) & (looks2 > 0))
    delta = np.full(t1.shape, np.nan, dtype='float32')
    delta[valid] = 10 * np.log10((t2[valid].astype('float64')+1e-10) / (t1[valid].astype('float64')+1e-10))
    return delta, valid

def read_aligned(out, names):
    arrays, profile = {}, None
    for name in names:
        with rasterio.open(Path(out) / (name+'.tif')) as src:
            key = (src.crs, src.transform, src.shape)
            if src.crs != rasterio.crs.CRS.from_string(CRS) or (profile is not None and key != profile):
                raise ValueError('Raster alignment or compute CRS differs')
            arrays[name], profile = src.read(1), key
    return arrays, profile[1]

def run(out=OUT):
    meta = read_json(Path(out) / 'subset-metadata.json')
    pair = meta['pair']
    comparable = pair.get('comparable', False) and all(checks(pair['t1'], pair['t2']).values())
    names = ['t1', 't2', 'mask_t1', 'mask_t2', 'looks_t1', 'looks_t2']
    a, affine = read_aligned(out, names)
    delta, valid = compute_change(*(a[n] for n in names),
        meta['sources']['t1']['number_of_subswaths'], meta['sources']['t2']['number_of_subswaths'], comparable)
    write_tif(Path(out) / 'delta_db.tif', delta, affine)
    print(f'{int(valid.sum())}/{valid.size} pixels passed quality gate')

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--out', type=Path, default=OUT)
    try:
        run(p.parse_args().out)
    except (ValueError, OSError, KeyError) as exc:
        p.exit(2, f'Blocked: {exc}\n')
