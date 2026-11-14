"""Consume Jelibeth's road vectors and OPERA overlap results; no acquisition."""
from datetime import datetime
import math
from rasterio.features import shapes
from shapely.geometry import shape, GeometryCollection
from shapely.ops import unary_union
from common import project, read_json

def load_roads(path, metadata):
    if path is None or metadata is None:
        raise ValueError('Real roads.geojson and osm-metadata.json required; missing roads cannot become zero metres')
    meta = read_json(metadata)
    if meta.get('attribution') != '© OpenStreetMap contributors' or not meta.get('query'):
        raise ValueError('OSM query and attribution required')
    ts = datetime.fromisoformat(meta['retrieved_at'].replace('Z', '+00:00'))
    if ts.tzinfo is None:
        raise ValueError('OSM retrieved_at requires timezone')
    geo = read_json(path)
    if geo.get('type') != 'FeatureCollection':
        raise ValueError('Roads must be a WGS84 FeatureCollection')
    lines = []
    for f in geo['features']:
        g = shape(f['geometry'])
        if g.geom_type not in ('LineString', 'MultiLineString') or not g.is_valid:
            raise ValueError('Invalid road geometry')
        if g.bounds and not (-180 <= g.bounds[0] <= g.bounds[2] <= 180 and -90 <= g.bounds[1] <= g.bounds[3] <= 90):
            raise ValueError('Road coordinates must be WGS84')
        lines.append(project(g))
    return unary_union(lines) if lines else GeometryCollection(), meta

def change_zone(changed, affine, buffer_m):
    polygons = [shape(g) for g, v in shapes(changed.astype('uint8'), mask=changed, transform=affine) if v == 1]
    return unary_union(polygons).buffer(buffer_m) if polygons else GeometryCollection()

def opera_context(path, pair, threshold, max_gap_days=6):
    """Read per-cell overlap computed by Jelibeth for this exact NISAR pair."""
    unavailable = {'status': 'unavailable', 't1': {'granules': [], 'gap_days': None},
                   't2': {'granules': [], 'gap_days': None}}
    if path is None:
        return {}, unavailable
    meta = read_json(path)
    if meta.get('status') == 'unavailable':
        return {}, unavailable
    if meta.get('status') != 'available':
        raise ValueError('OPERA status must be available or unavailable')
    if meta.get('product') != 'OPERA_L3_DSWx-S1' or meta.get('derived_from') != 'Sentinel-1':
        raise ValueError('Expected NASA OPERA DSWx-S1 derived from Sentinel-1')
    if not meta.get('class_mapping') or not meta.get('specification_url'):
        raise ValueError('Producer must document OPERA classes and product specification')
    if meta.get('nisar') != {label: pair[label]['granule'] for label in ('t1', 't2')} or meta.get('delta_db_threshold') != threshold:
        raise ValueError('OPERA overlaps must reference this NISAR pair and change threshold')
    manifest = {'status': 'available'}
    for label in ('t1', 't2'):
        side = meta[label]
        if not side['granules'] or not side.get('datetime'):
            raise ValueError('OPERA granules and timestamps required')
        dt = datetime.fromisoformat(side['datetime'].replace('Z', '+00:00'))
        nisar_dt = datetime.fromisoformat(pair[label]['datetime'].replace('Z', '+00:00'))
        if dt.tzinfo is None or nisar_dt.tzinfo is None:
            raise ValueError('OPERA and NISAR timestamps require timezone')
        gap = abs((dt-nisar_dt).total_seconds()) / 86400
        if gap > max_gap_days:
            return {}, unavailable
        # Store conservative integer days for the shared manifest contract.
        manifest[label] = {'granules': side['granules'], 'gap_days': math.ceil(gap)}
    overlaps = meta['cells']
    if not isinstance(overlaps, dict) or any(
        ratio is not None and (isinstance(ratio, bool) or not isinstance(ratio, (int, float))
                               or not math.isfinite(ratio) or not 0 <= ratio <= 1)
        for ratio in overlaps.values()
    ):
        raise ValueError('OPERA cell overlaps must be fractions in [0,1] or null')
    return overlaps, manifest
