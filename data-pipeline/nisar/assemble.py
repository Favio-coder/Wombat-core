"""Validate unsigned scientific files and stage an immutable Rust handoff.

Default staging is under data-pipeline, honoring the implementation scope.
This is not a signed/publishable bundle; Rust owns index, hashes and WONB.
"""
import argparse
import shutil
from pathlib import Path
import numpy as np
from PIL import Image
from common import OUT, ROOT, read_json, write_json
from contracts import validate

SCIENCE = ('manifest.json', 'cells.geojson', 'roads.geojson', 'provenance.json', 'risk-config.json',
           'overlays/t1.png', 'overlays/t2.png', 'overlays/delta.png', 'overlays/bounds.json', 'overlays/scales.json')

def verify_science(source):
    source = Path(source)
    missing = [name for name in SCIENCE if not (source/name).is_file()]
    if missing:
        raise ValueError('Missing scientific deliverables: '+', '.join(missing))
    manifest, cells = read_json(source/'manifest.json'), read_json(source/'cells.geojson')
    validate('manifest.schema.json', manifest)
    validate('cells.schema.json', cells)
    validate('risk-config.schema.json', read_json(source/'risk-config.json'))
    if not manifest['nisar']['comparable'] or not all(manifest['nisar']['comparable_checks'].values()):
        raise ValueError('Science requires comparable scenes')
    seen = set()
    for cell in cells['features']:
        p = cell['properties']
        if p['cell_id'] in seen:
            raise ValueError('Duplicate cell ID')
        seen.add(p['cell_id'])
        if not np.isclose(p['decrease_fraction']+p['increase_fraction'], p['changed_fraction']):
            raise ValueError('Inconsistent signed fractions')
        if p['road_m_near_change'] > p['road_m_total']:
            raise ValueError('Exposed road length exceeds total')
        if p['valid_fraction'] == 0 and p['p90_abs_delta_db'] is not None:
            raise ValueError('No valid pixels requires null p90')
    dimensions = set()
    for name in ('t1','t2','delta'):
        with Image.open(source/f'overlays/{name}.png') as im:
            im.verify()
        with Image.open(source/f'overlays/{name}.png') as im:
            dimensions.add(im.size)
            if im.mode != 'RGBA':
                raise ValueError('Overlays must include transparency')
    if len(dimensions) != 1:
        raise ValueError('Overlay grids differ')
    bounds = read_json(source/'overlays/bounds.json')
    if bounds['crs'] != 'EPSG:4326' or len(bounds['coordinates']) != 4:
        raise ValueError('Invalid overlay bounds')
    return {'science_valid': True, 'cells': len(seen), 'signed_bundle_valid': False,
            'remaining': ['Rust signed WONB blocks, ranking, trusted issuers, risk config, index and evidence chain',
                          'Review provisional AOI / active rice and urban exclusions (R13)']}

def stage(source, destination):
    source, destination = Path(source), Path(destination)
    result = verify_science(source)
    if destination.exists():
        raise FileExistsError('R28: existing version must never be overwritten')
    destination.mkdir(parents=True)
    for name in SCIENCE:
        target = destination/name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source/name, target)
    write_json(destination/'handoff-status.json', result)
    return result

if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--source', type=Path, default=OUT)
    p.add_argument('--destination', type=Path, default=ROOT/'staging/data/piura-01/v1')
    p.add_argument('--validate-only', action='store_true')
    a = p.parse_args()
    try:
        print(verify_science(a.source) if a.validate_only else stage(a.source, a.destination))
    except (ValueError, FileExistsError) as exc:
        p.exit(2, f'Blocked: {exc}\n')
