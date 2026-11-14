"""ASF metadata discovery. No downloads and no inference of missing metadata.

API: https://nisar-docs.asf.alaska.edu/asf-search/
Naming: https://nisar-docs.asf.alaska.edu/gcov/#naming-convention
"""
import argparse
from datetime import datetime, timezone
from pathlib import Path
from common import BBOX, OUT, write_json
from shapely.geometry import box

KEYS = ('frame', 'orbit_direction', 'mode', 'frequency', 'polarization', 'maturity')
MATURITIES = ('PROVISIONAL', 'VALIDATED', 'BETA')
COLLECTIONS = {'PROVISIONAL': 'NISAR_L2_GCOV_PROVISIONAL_V1',
               'VALIDATED': 'NISAR_L2_GCOV_V1', 'BETA': 'NISAR_L2_GCOV_BETA_V1'}

def checks(t1, t2):
    return {'same_' + k: bool(t1.get(k) and t2.get(k) and t1[k] == t2[k]) for k in KEYS}

def normalize(properties, maturity, frequency='A', polarization='HHHH'):
    name = properties.get('fileID') or properties.get('sceneName') or ''
    name = Path(name).stem
    parts = name.split('_')
    size = properties.get('bytes')
    if isinstance(size, dict):
        size = size.get(name+'.h5', {}).get('bytes')
    scene = dict(granule=name, datetime=properties.get('startTime'),
                 frame=None, orbit_direction=None, mode=None, frequency=None,
                 polarization=None, maturity=maturity, urls=[], size_bytes=size,
                 asf_properties=properties)
    # Filename supplies acquisition parameters, not product maturity (query supplies that).
    if len(parts) == 18 and parts[:1] == ['NISAR'] and parts[1] == 'L2' and parts[3] == 'GCOV':
        pol_code = parts[9][0:2] if frequency == 'A' else parts[9][2:4]
        pols = {'SH': ['HHHH'], 'SV': ['VVVV'], 'DH': ['HHHH', 'HVHV'],
                'DV': ['VVVV', 'VHVH'], 'QP': ['HHHH', 'HVHV', 'VVVV', 'VHVH'],
                'CR': ['RHRH', 'RVRV']}.get(pol_code, [])
        band = parts[8][:2] if frequency == 'A' else parts[8][2:]
        scene.update(frame=parts[5] + ':' + parts[7],
                     orbit_direction={'A': 'ascending', 'D': 'descending'}.get(parts[6]),
                     mode=parts[8], frequency=frequency if band != '00' else None,
                     polarization=polarization if polarization in pols else None)
        scene['datetime'] = datetime.strptime(parts[11], '%Y%m%dT%H%M%S').replace(tzinfo=timezone.utc).isoformat().replace('+00:00', 'Z')
    if properties.get('url'):
        scene['urls'].append(properties['url'])
    return scene

def select_pair(scenes, pivot='2026-10-20'):
    # Interpret date-only event as the whole UTC day; exclude October 20.
    day = datetime.fromisoformat(pivot).date()
    before = [s for s in scenes if s.get('datetime') and datetime.fromisoformat(s['datetime'].replace('Z', '+00:00')).date() < day]
    after = [s for s in scenes if s.get('datetime') and datetime.fromisoformat(s['datetime'].replace('Z', '+00:00')).date() > day]
    for maturity in MATURITIES:
        pairs = [(a, b) for a in before for b in after
                 if a['maturity'] == maturity and all(checks(a, b).values())]
        if pairs:
            # Latest T1 then earliest T2 within an identical acquisition configuration.
            a, b = min(pairs, key=lambda pair: (
                -datetime.fromisoformat(pair[0]['datetime'].replace('Z', '+00:00')).timestamp(),
                pair[1]['datetime'], pair[0]['granule'], pair[1]['granule']))
            return {'t1': a, 't2': b, 'comparable': True, 'comparable_checks': checks(a, b)}
    return None

def run(output=OUT / 'candidates.json', start='2026-10-01T00:00:00Z', end='2026-11-10T23:59:59Z', frequency='A', polarization='HHHH'):
    import asf_search as asf
    result = {'bbox_wgs84': BBOX, 'start': start, 'end': end, 'pivot_day_utc': '2026-10-20',
              'queried_at': datetime.now(timezone.utc).isoformat(), 'candidates': [], 'queries': [],
              'selected_pair': None, 'comparable': False, 'status': 'blocked'}
    for maturity in MATURITIES:
        # Collection shortName also supports ASF releases without dataMaturity.
        query = dict(shortName=COLLECTIONS[maturity],
                     intersectsWith=box(*BBOX).wkt, start=start, end=end)
        record = {'parameters': query}
        try:
            found = asf.search(**query)
            record['count'] = len(found)
            for item in found:
                scene = normalize(item.properties, maturity, frequency, polarization)
                scene['urls'] = list(dict.fromkeys(scene['urls'] + item.get_urls()))
                result['candidates'].append(scene)
        except Exception as exc:
            record['error'] = f'{type(exc).__name__}: {exc}'
        result['queries'].append(record)
    complete = all('error' not in query for query in result['queries'])
    pair = select_pair(result['candidates']) if complete else None
    result['cross_event_checks'] = [
        {'t1': a['granule'], 't2': b['granule'], 'comparable_checks': checks(a,b)}
        for a in result['candidates'] for b in result['candidates']
        if a['datetime'] and b['datetime'] and a['datetime'][:10] < '2026-10-20' < b['datetime'][:10]]
    result.update(selected_pair=pair, comparable=pair is not None,
                  status='pair_found' if pair else 'blocked',
                  reason=None if pair else ('No comparable pair returned.' if complete else 'ASF search incomplete; retry failed queries.'))
    write_json(output, result)
    print(f'{result["status"]}: {len(result["candidates"])} candidates; {output}')
    return result

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output', type=Path, default=OUT / 'candidates.json')
    p.add_argument('--frequency', choices=['A', 'B'], default='A')
    p.add_argument('--polarization', default='HHHH')
    args = p.parse_args()
    result = run(args.output, frequency=args.frequency, polarization=args.polarization)
    raise SystemExit(0 if result['comparable'] else 2)
