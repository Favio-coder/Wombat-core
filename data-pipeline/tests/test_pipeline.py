"""All manufactured arrays/IDs are TEST FIXTURES in pytest tmp_path, never NASA outputs."""
from copy import deepcopy
import h5py
import numpy as np
import pytest
from PIL import Image
from rasterio.transform import from_origin
from shapely.geometry import box, LineString
import cells
import subset
from common import collection, feature, project, read_json, write_json, ROOT
from contracts import validate
from jsonschema import ValidationError
from create_grid import make_grid
from check_grid import validate_grid
from search import normalize, checks, select_pair, run as search_run
from change import compute_change, run as change_run
from cells import metrics
from integration import load_roads, opera_context
from assemble import stage, verify_science
from subset import write_tif

def scene(day='07', maturity='PROVISIONAL'):
    # Illustrative naming test, not a catalog claim.
    name = f'NISAR_L2_PR_GCOV_001_112_D_091_4005_DHDH_A_202610{day}T120000_202610{day}T120030_P00000_N_F_J_001'
    return normalize({'fileID': name}, maturity)

def test_grid_roundtrip():
    grid = make_grid()
    assert validate_grid(grid)['cells'] == 812
    assert grid == make_grid()
    assert set(grid['features'][0]['properties']) == {'cell_id'}

@pytest.mark.parametrize('fault', ['missing', 'duplicate', 'crs', 'dimension', 'id', 'extra'])
def test_grid_rejects_corruption(fault):
    grid = deepcopy(make_grid())
    first = grid['features'][0]
    if fault == 'missing': grid['features'].pop(100)
    if fault == 'duplicate': grid['features'].append(deepcopy(first))
    if fault == 'crs': grid['crs']['properties']['name'] = 'EPSG:4326'
    if fault == 'dimension': first['geometry'] = feature(box(500000, 9000000, 500500, 9001000), {})['geometry']
    if fault == 'id': first['properties']['cell_id'] = 'piura-01-r99c99'
    if fault == 'extra': first['properties']['invented'] = 1
    with pytest.raises(ValueError): validate_grid(grid)

@pytest.mark.parametrize('key', ['frame','orbit_direction','mode','frequency','polarization','maturity'])
def test_each_comparability_check(key):
    a, b = scene(), scene('31')
    assert all(checks(a,b).values())
    b[key] = 'different'
    assert not checks(a,b)['same_'+key]
    assert select_pair([a,b]) is None
    b[key] = None
    assert not checks(b,b)['same_'+key]

def test_selection_maturity_and_dates():
    assert select_pair([scene('07'),scene('19'),scene('20'),scene('31')])['t1']['datetime'].startswith('2026-10-19')
    assert select_pair([scene('19'),scene('31','VALIDATED')]) is None
    assert select_pair([scene('19','VALIDATED'),scene('31','VALIDATED'),scene('07'),scene('25')])['t1']['maturity'] == 'PROVISIONAL'

def test_saved_catalog_comparability():
    catalog = read_json(ROOT/'out/piura-01/candidates.json')
    assert select_pair(catalog['candidates']) == catalog['selected_pair']
    assert catalog['comparable'] == (catalog['selected_pair'] is not None)

def test_normalize_missing_and_frequency():
    assert not all(checks(normalize({}, 'PROVISIONAL'), normalize({}, 'PROVISIONAL')).values())
    name = scene()['granule'].replace('4005_DHDH', '4000_DHNA')
    assert normalize({'fileID':name}, 'PROVISIONAL', 'B')['frequency'] is None
    assert normalize({'fileID':name}, 'PROVISIONAL', 'A','VVVV')['polarization'] is None

def test_search_empty_or_failure_is_not_comparable(tmp_path, monkeypatch):
    import asf_search
    monkeypatch.setattr(asf_search, 'search', lambda **kwargs: [])
    result = search_run(tmp_path/'empty.json')
    assert result['status'] == 'blocked' and result['comparable'] is False
    def fail(**kwargs): raise ConnectionError('test network failure')
    monkeypatch.setattr(asf_search, 'search', fail)
    result = search_run(tmp_path/'failed.json')
    assert all('error' in q for q in result['queries'])
    assert not result['comparable']

def test_quality_gate_signed_and_masks():
    one = np.ones((1,10), dtype='float32')
    t1, t2, m1, m2, l1, l2 = [one.copy() for _ in range(6)]
    t2[0,:2] = [.1,10]
    m1[0,2:5] = [0,255,4]
    l1[0,5], l2[0,6] = 0,np.nan
    t1[0,7], t2[0,8], t2[0,9] = -1,np.inf,0
    delta, valid = compute_change(t1,t2,m1,m2,l1,l2,3,3,True)
    assert valid.sum() == 2
    np.testing.assert_allclose(delta[0,:2], [-10,10])
    assert np.isnan(delta[0,2:]).all()
    with pytest.raises(ValueError): compute_change(t1,t2,m1,m2,l1,l2,3,3,False)

def test_metrics_denominators_and_no_data():
    delta = np.array([[-3,3,0,np.nan]], 'float32')
    m = metrics(delta, np.ones(delta.shape,bool))
    assert m['valid_fraction'] == .75
    assert m['changed_fraction'] == pytest.approx(2/3)
    assert m['decrease_fraction'] == m['increase_fraction'] == pytest.approx(1/3)
    empty = metrics(delta, np.zeros(delta.shape,bool))
    assert empty['p90_abs_delta_db'] is None and empty['valid_fraction'] == 0

def make_hdf(path, sc, increase=False):
    with h5py.File(path, 'w') as h:
        g = h.create_group('/science/LSAR/GCOV/grids/frequencyA')
        for name, value in {'projection':32717, 'xCoordinateSpacing':100., 'yCoordinateSpacing':-100.,
                            'xCoordinates':np.arange(522050,523000,100), 'yCoordinates':np.arange(9429950,9429000,-100),
                            'numberOfSubSwaths':3}.items(): g[name] = value
        g['HHHH'] = np.full((10,10), 10 if increase else 1, 'float32')
        g['mask'] = np.ones((10,10),'uint8')
        g['numberOfLooks'] = np.ones((10,10),'float32')
        g['inputDataExceptionMask'] = np.zeros((10,10),'uint8')
        g['inputDataExceptionMask'][0,0] = 2
        ident = h.create_group('/science/LSAR/identification')
        for name, value in {'productType':'GCOV','trackNumber':112,'frameNumber':91,
                            'orbitPassDirection':'Descending','granuleId':sc['granule'],
                            'zeroDopplerStartTime':sc['datetime'], 'productSpecificationVersion':'1.5.0'}.items():
            ident[name] = value

def test_hdf_window_and_path_validation(tmp_path):
    sc = scene()
    path = tmp_path/(sc['granule']+'.h5')
    make_hdf(path,sc)
    arrays, affine, crs, meta = subset.read_window(path,(522200,9429200,522400,9429400))
    assert arrays['HHHH'].shape[0] < 10 and arrays['HHHH'].shape[1] < 10
    assert crs == 'EPSG:32717'
    subset.validate_source(sc,path,meta)
    meta['frameNumber'] = 92
    with pytest.raises(ValueError): subset.validate_source(sc,path,meta)
    with pytest.raises(ValueError): subset.read_window(path,(0,0,10,10))
    with pytest.raises(ValueError): subset.read_window(path,(522200,9429200,522400,9429400),polarization='HHHV')

def test_missing_roads_and_opera(tmp_path):
    with pytest.raises(ValueError): load_roads(None,None)
    context, meta = opera_context(None,{'t1':scene(),'t2':scene('31')},3)
    assert context == {} and meta['status'] == 'unavailable'

@pytest.mark.parametrize('exclude_landcover', [False, True])
def test_end_to_end_unsigned_science(tmp_path, monkeypatch, exclude_landcover):
    """Manufactured HDF5→subset→gate→cells→PNG→schema→immutable staging."""
    inp, out = tmp_path/'inputs', tmp_path/'out'
    inp.mkdir()
    g = box(522000,9429000,523000,9430000)
    write_json(inp/'grid.geojson',collection([feature(g,{'cell_id':'piura-01-r00c00'})]))
    monkeypatch.setattr(subset,'INPUT',inp)
    monkeypatch.setattr(cells,'INPUT',inp)
    monkeypatch.setattr(subset,'aoi',lambda:g)
    a,b = scene(),scene('31')
    pair = {'t1':a,'t2':b,'comparable':True,'comparable_checks':checks(a,b)}
    candidates = tmp_path/'candidates.json'
    write_json(candidates,{'selected_pair':pair})
    paths = [tmp_path/(s['granule']+'.h5') for s in (a,b)]
    make_hdf(paths[0],a)
    make_hdf(paths[1],b,True)
    exclusion = None
    if exclude_landcover:
        exclusion = inp/'landcover.tif'
        raw = np.zeros((10,10),'uint8')
        raw[-1,:] = 1
        write_tif(exclusion,raw,from_origin(522000,9430000,100,100),nodata=255)
    subset.run(candidates,*paths,out,exclusion)
    change_run(out)
    roads, osm = inp/'roads.geojson', inp/'osm-metadata.json'
    write_json(roads,collection([feature(project(LineString([(522000,9429500),(523000,9429500)]),'EPSG:32717','EPSG:4326'),{})]))
    write_json(osm,{'query':'TEST FIXTURE ONLY','retrieved_at':'2026-11-14T00:00:00Z','attribution':'© OpenStreetMap contributors'})
    opera = None
    if exclude_landcover:
        opera = inp/'opera-context.json'
        write_json(opera, {'status':'available', 'product':'OPERA_L3_DSWx-S1', 'derived_from':'Sentinel-1',
            'specification_url':'https://example.test/TEST_ONLY', 'class_mapping':{'TEST_ONLY':'water'},
            'nisar':{label:pair[label]['granule'] for label in ('t1','t2')},
            'delta_db_threshold':3, 'cells':{'piura-01-r00c00':.3},
            **{label:{'granules':['TEST_ONLY'], 'datetime':pair[label]['datetime']} for label in ('t1','t2')}})
    cells.run(out,roads,osm,opera)
    result = verify_science(out)
    assert result['science_valid'] and not result['signed_bundle_valid']
    p = read_json(out/'cells.geojson')['features'][0]['properties']
    assert p['valid_fraction'] == (.89 if exclude_landcover else .99)
    assert p['increase_fraction'] == 1 and p['decrease_fraction'] == 0
    assert p['road_m_total'] == p['road_m_near_change'] == 1000
    assert p['opera_status'] == ('corroborated' if exclude_landcover else 'unavailable')
    invalid = read_json(out/'manifest.json')
    for bad_time in ('not-a-date', '20261007T120000Z', '2026-10-07T12:00:00'):
        invalid['nisar']['t1']['datetime'] = bad_time
        with pytest.raises(ValidationError): validate('manifest.schema.json',invalid)
    scale = read_json(out/'overlays/scales.json')
    assert scale['delta_db'] == [-6,6]
    with Image.open(out/'overlays/delta.png') as im:
        rgba = np.asarray(im)
        assert (rgba[...,3] == 0).any()
        assert (rgba[...,0][rgba[...,3]>0] == 255).all()
        assert (rgba[...,2][rgba[...,3]>0] == 0).all()
    target = tmp_path/'staging/data/piura-01/v1'
    stage(out,target)
    assert not (target/'index.json').exists() and not (target/'wonb').exists()
    with pytest.raises(FileExistsError): stage(out,target)

def test_opera_results_and_temporal_gate(tmp_path):
    pair = {'t1':scene(),'t2':scene('31')}
    meta = {'status':'available','product':'OPERA_L3_DSWx-S1','derived_from':'Sentinel-1',
            'specification_url':'https://example.test/spec-TEST-ONLY','class_mapping':{'TEST_ONLY':'decoded by producer'},
            'nisar':{label:pair[label]['granule'] for label in ('t1','t2')},
            'delta_db_threshold':3, 'cells':{'piura-01-r00c00':.3, 'piura-01-r00c01':None},
            't1':{'granules':['TEST_ONLY_1'],'datetime':'2026-10-07T12:00:00Z'},
            't2':{'granules':['TEST_ONLY_2'],'datetime':'2026-10-31T12:00:00Z'}}
    context_path = tmp_path/'opera-context.json'
    write_json(context_path,meta)
    context, result = opera_context(context_path,pair,3)
    assert result['status'] == 'available' and context == meta['cells']
    with pytest.raises(ValueError): opera_context(context_path,pair,4)
    meta['cells']['piura-01-r00c00'] = 2
    write_json(context_path,meta)
    with pytest.raises(ValueError): opera_context(context_path,pair,3)
    meta['cells']['piura-01-r00c00'] = .3
    meta['t2']['datetime'] = '2026-11-07T12:00:00Z'
    write_json(context_path,meta)
    context, result = opera_context(context_path,pair,3)
    assert context == {} and result['status'] == 'unavailable'

def test_alignment_gate(tmp_path):
    from change import read_aligned
    write_tif(tmp_path/'t1.tif',np.ones((2,2),'float32'),from_origin(522000,9430000,20,20))
    write_tif(tmp_path/'t2.tif',np.ones((2,2),'float32'),from_origin(522001,9430000,20,20))
    with pytest.raises(ValueError): read_aligned(tmp_path,['t1','t2'])
