"""Cell science, independent context, schema validation; no risk score."""
import argparse
import subprocess
import shutil
from pathlib import Path
import numpy as np
from rasterio.features import geometry_mask
from shapely.geometry import shape, mapping
from common import AOI_ID, BBOX, CRS, INPUT, OUT, ROOT, collection, feature, project, read_json, write_json
from change import read_aligned
from search import checks
from contracts import validate
from integration import load_roads, change_zone, opera_context
from overlays import render

def metrics(delta, region, threshold=3):
    valid = region & np.isfinite(delta)
    count, total = int(valid.sum()), int(region.sum())
    d = delta[valid]
    return {'valid_fraction': count/total if total else 0,
            'decrease_fraction': float(np.count_nonzero(d <= -threshold)/count) if count else 0,
            'increase_fraction': float(np.count_nonzero(d >= threshold)/count) if count else 0,
            'changed_fraction': float(np.count_nonzero(np.abs(d) >= threshold)/count) if count else 0,
            'p90_abs_delta_db': float(np.percentile(np.abs(d),90)) if count else None}

def run(out=OUT, roads=None, osm_metadata=None, opera=None, config=ROOT/'config/risk-config.json'):
    out = Path(out)
    meta = read_json(out / 'subset-metadata.json')
    pair = meta['pair']
    cfg = read_json(config)
    validate('risk-config.schema.json', cfg)
    threshold = cfg['delta_db_threshold']
    comparable = checks(pair['t1'], pair['t2'])
    if not pair.get('comparable') or not all(comparable.values()):
        raise ValueError('R6: incomparable source pair')
    road_geom, osm = load_roads(roads, osm_metadata)
    a, affine = read_aligned(out, ['t1', 't2', 'delta_db'])
    delta = a['delta_db']
    zone = change_zone(np.isfinite(delta) & (np.abs(delta) >= threshold), affine, cfg['road_buffer_meters'])
    context, opera_meta = opera_context(opera, pair, threshold, cfg['opera_max_gap_days'])
    features = []
    for cell in read_json(INPUT / 'grid.geojson')['features']:
        g = shape(cell['geometry'])
        region = geometry_mask([mapping(g)], out_shape=delta.shape, transform=affine, invert=True)
        props = {'cell_id': cell['properties']['cell_id'], **metrics(delta, region, threshold)}
        props.update(opera_status='unavailable', opera_overlap_ratio=None,
                     road_m_total=int(np.floor(road_geom.intersection(g).length+0.5)),
                     road_m_near_change=int(np.floor(road_geom.intersection(zone).intersection(g).length+0.5)))
        ratio = context.get(props['cell_id'])
        if ratio is not None:
            props.update(opera_overlap_ratio=ratio, opera_status='corroborated' if ratio >= cfg['corroboration_overlap_threshold'] else 'not_corroborated')
        features.append(feature(project(g, CRS, 'EPSG:4326'), props))
    cells = collection(features, crs_note='EPSG:4326 geometry; calculations EPSG:32717. Full cell denominator; change fractions among valid pixels.')
    t1 = pair['t1']
    nisar = {k: t1[k] for k in ('maturity','frame','orbit_direction','mode','frequency','polarization')}
    nisar.update(product='NISAR_L2_GCOV', pixel_spacing_m=meta['pixel_spacing_m'], comparable=True,
                 comparable_checks=comparable,
                 **{label: {k: pair[label][k] for k in ('granule','datetime')} for label in ('t1','t2')})
    commit = subprocess.check_output(['git','rev-parse','HEAD'], cwd=ROOT.parent, text=True).strip()
    manifest = {'aoi_id': AOI_ID, 'aoi_name': 'Bajo Piura (AOI provisional)', 'data_origin': 'nisar_real',
                'crs_compute': CRS, 'bbox_wgs84': BBOX, 'nisar': nisar,
                'opera': {'product': 'OPERA_L3_DSWx-S1', 'derived_from': 'Sentinel-1', **opera_meta},
                'osm': {k: osm[k] for k in ('query','retrieved_at','attribution')},
                'pipeline': {'commit': commit, 'risk_config_version': cfg['version']}}
    validate('cells.schema.json', cells)
    validate('manifest.schema.json', manifest)
    render(out, a['t1'], a['t2'], delta, affine)
    write_json(out / 'cells.geojson', cells)
    write_json(out / 'manifest.json', manifest)
    if Path(roads).resolve() != (out/'roads.geojson').resolve():
        shutil.copyfile(roads, out/'roads.geojson')
    if Path(config).resolve() != (out/'risk-config.json').resolve():
        shutil.copyfile(config, out/'risk-config.json')
    # Sidecar preserves source URLs and timestamps without altering shared contracts (R12).
    write_json(out / 'provenance.json', {'sources': meta, 'osm': osm,
        'opera': read_json(opera) if opera else {'status': 'unavailable'},
        'quality_min_valid_fraction': cfg['quality_min_valid_fraction'], 'delta_db_threshold': threshold,
        'road_buffer_meters': cfg['road_buffer_meters']})
    print(f'Validated science: {len(features)} cells; signed bundle still requires Rust')

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--out', type=Path, default=OUT)
    p.add_argument('--roads', type=Path)
    p.add_argument('--osm-metadata', type=Path)
    p.add_argument('--opera', type=Path)
    p.add_argument('--config', type=Path, default=ROOT/'config/risk-config.json')
    a = p.parse_args()
    try:
        run(a.out, a.roads, a.osm_metadata, a.opera, a.config)
    except (ValueError, OSError, KeyError) as exc:
        p.exit(2, f'Blocked: {exc}\n')
