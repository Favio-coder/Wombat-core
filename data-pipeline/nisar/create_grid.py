"""Generate complete 1-km squares, north-to-south rows, west-to-east columns."""
import argparse
import math
from pathlib import Path
from shapely.geometry import box
from common import AOI_ID, BBOX, CRS, INPUT, aoi, collection, feature, write_json

def make_grid():
    area = aoi()
    left, bottom, right, top = area.bounds
    left, bottom = math.floor(left / 1000) * 1000, math.floor(bottom / 1000) * 1000
    right, top = math.ceil(right / 1000) * 1000, math.ceil(top / 1000) * 1000
    cells = []
    for row, north in enumerate(range(top, bottom, -1000)):
        for col, west in enumerate(range(left, right, 1000)):
            cell = box(west, north - 1000, west + 1000, north)
            if cell.intersects(area):
                cells.append(feature(cell, {'cell_id': f'{AOI_ID}-r{row:02d}c{col:02d}'}))
    return collection(cells, crs={'type': 'name', 'properties': {'name': CRS}},
                      crs_note='Internal projected GeoJSON; EPSG:32717 metres. Not RFC 7946. Full 1000 m squares; NW origin.')

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output', type=str, default=str(INPUT))
    args = p.parse_args()
    out = Path(args.output)
    write_json(out / 'aoi.geojson', collection([feature(box(*BBOX), {'aoi_id': AOI_ID})]))
    grid = make_grid()
    write_json(out / 'grid.geojson', grid)
    print(f'Generated {len(grid["features"])} complete cells in {CRS}: {out}')

if __name__ == '__main__':
    main()
