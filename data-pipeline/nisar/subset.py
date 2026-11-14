"""Windowed reads of local NASA GCOV HDF5, then a common UTM grid.

Paths and mask: JPL D-102274 Rev F, tables in sections 5.2-5.3
(mask: printed pp. 36 and 44; official URL in README.md).
Only diagonal covariance terms are gamma-zero power. No off-diagonal terms.
"""
import argparse
from datetime import datetime, timezone
from pathlib import Path
import h5py
import numpy as np
import rasterio
from rasterio.transform import Affine, from_origin
from rasterio.warp import reproject, Resampling, transform_bounds
from rasterio.features import geometry_mask
from shapely.geometry import mapping, shape
from shapely.ops import unary_union
from common import CRS, INPUT, OUT, aoi, read_json, write_json
from search import checks, normalize

DIAGONAL = {'HHHH', 'HVHV', 'VVVV', 'VHVH', 'RHRH', 'RVRV'}

def value(ds):
    v = ds[()]
    if isinstance(v, bytes):
        return v.decode('utf-8')
    if isinstance(v, np.ndarray):
        return [x.decode() if isinstance(x, bytes) else x.item() if hasattr(x, 'item') else x for x in v.flat]
    return v.item() if hasattr(v, 'item') else v

def read_window(path, bounds, frequency='A', polarization='HHHH'):
    if polarization not in DIAGONAL:
        raise ValueError('Only real diagonal covariance terms are gamma-zero power')
    with h5py.File(path, 'r') as h:
        group_path = f'/science/LSAR/GCOV/grids/frequency{frequency}'
        g = h[group_path]
        epsg = int(g['projection'][()])
        x, y = g['xCoordinates'][:], g['yCoordinates'][:]
        dx, dy = float(g['xCoordinateSpacing'][()]), float(g['yCoordinateSpacing'][()])
        if dx <= 0 or dy >= 0 or len(x) < 2 or len(y) < 2:
            raise ValueError('Expected regular eastward x and southward y coordinates')
        if not np.allclose(np.diff(x), dx) or not np.allclose(np.diff(y), dy):
            raise ValueError('Irregular coordinates')
        src_crs = f'EPSG:{epsg}'
        west, south, east, north = transform_bounds(CRS, src_crs, *bounds, densify_pts=21)
        # Include a one-pixel halo. HDF5 image layers are never read in full.
        cols = np.flatnonzero((x >= west-dx) & (x <= east+dx))
        rows = np.flatnonzero((y >= south+dy) & (y <= north-dy))
        if not len(rows) or not len(cols):
            raise ValueError('Granule has no intersection with AOI')
        r0, r1, c0, c1 = int(rows[0]), int(rows[-1])+1, int(cols[0]), int(cols[-1])+1
        window = np.s_[r0:r1, c0:c1]
        expected = (len(y), len(x))
        arrays = {}
        for name in (polarization, 'mask', 'numberOfLooks'):
            ds = g[name]
            if ds.shape != expected:
                raise ValueError(f'{name}: unexpected dimensions {ds.shape}')
            arr = ds[window]
            if name != 'mask':
                arr = arr.astype('float32')
                fill = ds.attrs.get('_FillValue')
                if fill is not None:
                    arr[arr == fill] = np.nan
            arrays[name] = arr
        if 'inputDataExceptionMask' in g:
            exceptions = g['inputDataExceptionMask'][window]
            arrays[polarization][exceptions != 0] = np.nan
        swaths = int(g['numberOfSubSwaths'][()])
        if not 1 <= swaths <= 254:
            raise ValueError('Invalid numberOfSubSwaths')
        ident = h['/science/LSAR/identification']
        metadata = {name: value(ds) for name, ds in ident.items() if isinstance(ds, h5py.Dataset) and ds.size < 100}
        metadata.update(source_path=str(Path(path).resolve()), source_size_bytes=Path(path).stat().st_size,
                        extracted_at=datetime.now(timezone.utc).isoformat(),
                        input_exception_mask_applied='inputDataExceptionMask' in g,
                        group_path=group_path, datasets=list(arrays), number_of_subswaths=swaths,
                        source_crs=src_crs, x_spacing_m=dx, y_spacing_m=dy,
                        hdf5_window=[r0, r1, c0, c1],
                        mask_description=str(g['mask'].attrs.get('description', '')))
        affine = Affine(dx, 0, x[c0]-dx/2, 0, dy, y[r0]-dy/2)
        return arrays, affine, src_crs, metadata

def write_tif(path, array, transform, crs=CRS, nodata=np.nan):
    with rasterio.open(path, 'w', driver='GTiff', height=array.shape[0], width=array.shape[1],
                       count=1, dtype=array.dtype, crs=crs, transform=transform, nodata=nodata,
                       compress='deflate') as dst:
        dst.write(array, 1)

def target_grid(spacing):
    grid = read_json(INPUT / 'grid.geojson')
    bounds = unary_union([shape(f['geometry']) for f in grid['features']]).bounds
    w, s, e, n = bounds
    width, height = int(np.ceil((e-w)/spacing)), int(np.ceil((n-s)/spacing))
    return bounds, from_origin(w, n, spacing, spacing), (height, width)

def validate_source(scene, path, meta):
    if Path(path).stem != scene['granule']:
        raise ValueError('Local file basename must match selected real granule ID')
    parsed = normalize({'fileID': Path(path).stem}, scene['maturity'], scene['frequency'], scene['polarization'])
    if not all(checks(scene, parsed).values()) or parsed['datetime'] != scene['datetime']:
        raise ValueError('Source filename does not match selected acquisition')
    required = {'productType': 'GCOV', 'orbitPassDirection': scene['orbit_direction']}
    for key, expected in required.items():
        if str(meta.get(key, '')).lower() != expected.lower():
            raise ValueError(f'HDF5 identification {key} disagrees with candidate')
    if str(meta.get('frameNumber', '')).zfill(3) != scene['frame'].split(':')[1]:
        raise ValueError('HDF5 frameNumber disagrees with candidate')
    if str(meta.get('trackNumber', '')).zfill(3) != scene['frame'].split(':')[0]:
        raise ValueError('HDF5 trackNumber disagrees with candidate')
    if Path(str(meta.get('granuleId', ''))).stem != scene['granule']:
        raise ValueError('HDF5 granuleId disagrees with candidate')
    source_time = str(meta.get('zeroDopplerStartTime', ''))
    if source_time[:19] != scene['datetime'][:19]:
        raise ValueError('HDF5 acquisition timestamp disagrees with candidate')

def run(candidates, t1, t2, out=OUT, exclusion_mask=None):
    pair = read_json(candidates).get('selected_pair')
    if not pair or not pair.get('comparable') or not all(checks(pair['t1'], pair['t2']).values()):
        raise ValueError('R6: no independently comparable selected pair')
    prepared = []
    for label, path in [('t1', t1), ('t2', t2)]:
        scene = pair[label]
        # Use source spacing, verified for both dates; no artificial increase in resolution.
        with h5py.File(path, 'r') as h:
            spacing = float(h[f'/science/LSAR/GCOV/grids/frequency{scene["frequency"]}/xCoordinateSpacing'][()])
        if not spacing.is_integer() or spacing <= 0:
            raise ValueError('Contract requires a positive integer pixel spacing')
        bounds, affine, dims = target_grid(spacing)
        arrays, source_transform, source_crs, meta = read_window(path, bounds, scene['frequency'], scene['polarization'])
        validate_source(scene, path, meta)
        if not np.isclose(abs(meta['y_spacing_m']), spacing):
            raise ValueError('Non-square source pixels')
        prepared.append((label, scene, spacing, affine, dims, arrays, source_transform, source_crs, meta))
    if prepared[0][2:5] != prepared[1][2:5]:
        raise ValueError('T1/T2 spacing or target grid differs')
    out = Path(out)
    out.mkdir(parents=True, exist_ok=True)
    source_metadata = {}
    exclusion_info = 'not_applied; R13 review required before publication'
    for label, scene, spacing, affine, dims, arrays, src_affine, src_crs, meta in prepared:
        inside = geometry_mask([mapping(aoi())], out_shape=dims, transform=affine, invert=True)
        if exclusion_mask is not None:
            exclusion = np.full(dims, 255, 'uint8')
            with rasterio.open(exclusion_mask) as src:
                raw = src.read(1)
                if src.crs is None or not set(np.unique(raw)).issubset({0,1,255}):
                    raise ValueError('Landcover exclusion raster: CRS required, 0=keep, 1=exclude, 255=unknown')
                reproject(raw, exclusion, src_transform=src.transform, src_crs=src.crs, src_nodata=255,
                          dst_transform=affine, dst_crs=CRS, dst_nodata=255, resampling=Resampling.nearest)
            inside &= exclusion == 0
            exclusion_info = str(Path(exclusion_mask).resolve())
        for src_name, dst_name in [(scene['polarization'], label), ('mask', 'mask_'+label), ('numberOfLooks', 'looks_'+label)]:
            is_mask = src_name == 'mask'
            nodata = 255 if is_mask else np.nan
            dst = np.full(dims, nodata, dtype='uint8' if is_mask else 'float32')
            reproject(arrays[src_name], dst, src_transform=src_affine, src_crs=src_crs,
                      dst_transform=affine, dst_crs=CRS, src_nodata=nodata, dst_nodata=nodata,
                      resampling=Resampling.nearest)
            dst[~inside] = nodata
            write_tif(out / (dst_name+'.tif'), dst, affine, nodata=nodata)
        source_metadata[label] = meta
    write_json(out / 'subset-metadata.json', {'pair': pair, 'sources': source_metadata,
               'pixel_spacing_m': spacing, 'resampling': 'nearest', 'crs': CRS,
               'aoi_provisional': True, 'landcover_exclusion': exclusion_info})
    print(f'Subsets written: {out}')

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--candidates', type=Path, default=OUT / 'candidates.json')
    p.add_argument('--t1', type=Path, required=True)
    p.add_argument('--t2', type=Path, required=True)
    p.add_argument('--out', type=Path, default=OUT)
    p.add_argument('--exclusion-mask', type=Path)
    a = p.parse_args()
    try:
        run(a.candidates, a.t1, a.t2, a.out, a.exclusion_mask)
    except (ValueError, OSError, KeyError) as exc:
        p.exit(2, f'Blocked: {exc}\n')
