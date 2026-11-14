"""Shared I/O and fixed provisional AOI. No signing or ranking."""
import json
from pathlib import Path
from pyproj import Transformer
from shapely.geometry import box, mapping
from shapely import transform

ROOT = Path(__file__).resolve().parents[1]
BBOX = [-80.80, -5.35, -80.55, -5.10]
AOI_ID = 'piura-01'
CRS = 'EPSG:32717'
INPUT = ROOT / 'inputs' / AOI_ID
OUT = ROOT / 'out' / AOI_ID

def read_json(path):
    return json.loads(Path(path).read_text(encoding='utf-8'))

def write_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False, allow_nan=False) + '\n', encoding='utf-8')

def project(geom, source='EPSG:4326', target=CRS):
    return transform(geom, Transformer.from_crs(source, target, always_xy=True).transform, interleaved=False)

def aoi():
    return project(box(*BBOX))

def feature(geom, props):
    # Shapely mappings contain tuples; JSON Schema arrays require JSON lists.
    return {'type': 'Feature', 'properties': props, 'geometry': json.loads(json.dumps(mapping(geom)))}

def collection(features, **extra):
    return {'type': 'FeatureCollection', **extra, 'features': features}
