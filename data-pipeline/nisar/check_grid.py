"""Validate geometry, dimensions, alignment, coverage and deterministic IDs."""
import argparse
import math
from shapely.geometry import shape, box
from shapely.ops import unary_union
from common import AOI_ID, CRS, INPUT, aoi, read_json

def validate_grid(grid):
    if grid.get('crs', {}).get('properties', {}).get('name') != CRS:
        raise ValueError('Grid CRS must explicitly be EPSG:32717')
    area = aoi()
    west = math.floor(area.bounds[0] / 1000) * 1000
    north = math.ceil(area.bounds[3] / 1000) * 1000
    seen, geometries = set(), []
    for f in grid['features']:
        g, props = shape(f['geometry']), f['properties']
        if set(props) != {'cell_id'} or not g.is_valid or g.geom_type != 'Polygon':
            raise ValueError('Invalid geometry or properties')
        x0, y0, x1, y1 = g.bounds
        if not (g.equals(box(*g.bounds)) and abs(x1-x0-1000) < 1e-6 and abs(y1-y0-1000) < 1e-6):
            raise ValueError('Cells must be complete 1000 x 1000 m squares')
        if any(abs(v / 1000 - round(v / 1000)) > 1e-9 for v in g.bounds):
            raise ValueError('Unaligned cell')
        row, col = round((north-y1)/1000), round((x0-west)/1000)
        expected = f'{AOI_ID}-r{row:02d}c{col:02d}'
        if props['cell_id'] != expected or expected in seen or not g.intersects(area):
            raise ValueError('Incorrect/duplicate ID or cell outside AOI')
        seen.add(expected)
        geometries.append(g)
    union = unary_union(geometries)
    if not geometries or area.difference(union).area > 1e-5:
        raise ValueError('Grid does not cover AOI')
    # Exact expected membership also catches missing boundary-touch cells.
    from create_grid import make_grid
    if seen != {f['properties']['cell_id'] for f in make_grid()['features']}:
        raise ValueError('Grid membership differs from AOI intersection')
    return {'cells': len(seen), 'crs': CRS, 'cell_area_m2': 1000000, 'valid': True}

if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('grid', nargs='?', default=str(INPUT / 'grid.geojson'))
    print(validate_grid(read_json(p.parse_args().grid)))
