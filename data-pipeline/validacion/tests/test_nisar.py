"""Pruebas con datos sinteticos. Correr:  python -m pytest data-pipeline/validacion -q"""
import json
import subprocess
import sys
from pathlib import Path

import numpy as np
import pytest
from PIL import Image

from wombat_nisar import change, contract
from wombat_nisar.grid import Grid
from wombat_nisar.manifest import build_manifest, check_comparable, scene_from_granule
from wombat_nisar.gcov import parse_granule_name
from wombat_nisar.synthetic import make_scene

NISAR = Path(__file__).resolve().parent.parent
# Nombre real tomado de la guia de ASF (https://nisar-docs.asf.alaska.edu/gdal/)
GRANULE = ("NISAR_L2_PR_GCOV_005_149_A_024_4005_DHDH_A_20251120T123755_"
           "20251120T123830_X05009_N_F_J_001")


def _run(script, *args):
    return subprocess.run([sys.executable, str(NISAR / script), *map(str, args)],
                          capture_output=True, text=True, cwd=NISAR)


def test_parse_granule_name():
    g = parse_granule_name(GRANULE + ".h5")
    assert (g["track"], g["frame"], g["orbit_direction"]) == ("149", "024", "ascending")
    assert (g["mode"], g["pol_code"]) == ("4005", "DHDH")
    assert g["datetime"] == "2025-11-20T12:37:55Z"
    with pytest.raises(ValueError):
        parse_granule_name("S1A_IW_GRDH_1SDV_20261012.h5")


def test_comparable_rules_and_manifest_schema():
    t1 = scene_from_granule(GRANULE, "PROVISIONAL")
    t2 = dict(t1, datetime="2025-12-02T12:37:55Z")
    assert check_comparable(t1, t2)[0] is True
    for field, value, check in [
        ("frame", "025", "same_frame"), ("track", "150", "same_frame"),
        ("orbit_direction", "descending", "same_orbit_direction"),
        ("mode", "2005", "same_mode"), ("frequency", "B", "same_frequency"),
        ("polarization", "HVHV", "same_polarization"), ("maturity", "BETA", "same_maturity"),
    ]:
        ok, checks = check_comparable(t1, dict(t2, **{field: value}))
        assert ok is False and checks[check] is False, field

    osm = {"query": "highway=*", "retrieved_at": "2026-11-14T15:00:00Z",
           "attribution": "© OpenStreetMap contributors"}
    args = dict(aoi_id="piura-01", aoi_name="x", bbox_wgs84=[-80.8, -5.35, -80.55, -5.1],
                t1=t1, pixel_spacing_m=20)
    good = dict(build_manifest(t2=t2, **args), osm=osm)
    assert contract.errors("manifest", good) == []
    assert contract.errors("manifest", build_manifest(t2=t2, **args))      # sin osm no cumple
    mixed = dict(build_manifest(t2=dict(t2, maturity="BETA"), **args), osm=osm)
    assert mixed["nisar"]["comparable"] is False                           # R7
    assert contract.errors("manifest", mixed) == []                        # valido, pero no se procesa


def test_delta_db_keeps_sign_and_respects_quality_gate():
    g1 = np.array([[0.08, 0.08, 0.08, 0.08, 0.0, np.nan]], dtype=np.float32)
    g2 = np.array([[0.02, 0.32, 0.08, 0.02, 0.02, 0.02]], dtype=np.float32)
    mask = np.array([[1, 1, 1, 0, 1, 1]], dtype=np.uint8)
    looks = np.ones((1, 6), dtype=np.float32)
    valid = change.quality_gate(g1, g2, mask, mask, looks, looks, [1, 2, 3, 4, 5])
    assert valid.tolist() == [[True, True, True, False, False, False]]
    delta = change.delta_db(g1, g2, valid)
    assert delta[0, 0] == pytest.approx(-6.02, abs=0.01)   # bajo: negativo
    assert delta[0, 1] == pytest.approx(+6.02, abs=0.01)   # subio: positivo
    assert delta[0, 2] == pytest.approx(0.0, abs=0.01)
    assert np.isnan(delta[0, 3:]).all()
    assert change.changed_mask(delta, 3.0).tolist() == [[1, 1, 0, 255, 255, 255]]
    assert change.quality_gate(g1, g2, mask, mask, looks * 0, looks, [1]).sum() == 0
    assert change.summary(np.full((4, 4), np.nan, np.float32), 3.0)["warnings"]
    flat = change.summary(np.zeros((4, 4), np.float32), 3.0)
    assert any(w.startswith("TODO ~0") for w in flat["warnings"])


def test_multilook_removes_speckle_and_aligns_to_grid():
    from rasterio.transform import from_origin
    rng = np.random.default_rng(3)
    shape = (100, 150)
    g1 = (0.08 * rng.gamma(8, 1 / 8, shape)).astype(np.float32)       # 8 looks, sin cambio real
    g2 = (0.08 * rng.gamma(8, 1 / 8, shape)).astype(np.float32)
    g2[:50, :50] *= 0.2                                                # un bloque que si bajo (-7 dB)
    valid = np.ones(shape, dtype=bool)
    valid[:, 140:] = False                                             # borde sin dato
    transform = from_origin(538000, 9428000, 20, 20)

    native = change.delta_db(g1, g2, valid)
    noisy = change.classify(native[50:, :140], 3.0)[2].mean()
    assert noisy > 0.10                                                # el ruido solo ya "cambia" >10 %
    assert change.false_change_fraction(3.0, change.speckle_std_db(8)) == pytest.approx(noisy, abs=0.05)

    m1, m2, ok, t, k = change.multilook(g1, g2, valid, transform, 100)
    assert k == 5 and m1.shape == (20, 30) and (t.a, t.c, t.f) == (100, 538000, 9428000)
    delta = change.delta_db(m1, m2, ok)
    assert not ok[:, 28:].any() and np.isnan(delta[:, 28:]).all()      # bloques sin dato
    assert change.classify(delta[10:, :28], 3.0)[2].mean() == 0        # ya no hay falso cambio
    assert np.all(delta[:10, :10] < -6)                                # el cambio real se conserva
    assert change.false_change_fraction(3.0, change.speckle_std_db(8 * 25)) < 1e-6

    # Una ventana que no empieza en un multiplo de 100 m se rellena para calzar con la grilla
    _, _, ok2, t2, _ = change.multilook(g1, g2, valid, from_origin(538040, 9427980, 20, 20), 100)
    assert (t2.c, t2.f) == (538000, 9428000) and ok2.shape == (21, 31)
    with pytest.raises(ValueError):
        change.multilook(g1, g2, valid, transform, 90)                 # 90 m no es multiplo de 20 m


def test_grid_is_aligned_and_round_trips(tmp_path):
    grid = Grid.from_bbox("piura-01", [-80.80, -5.35, -80.55, -5.10])
    assert grid.x0 % 1000 == 0 and grid.y_top % 1000 == 0
    west, south, east, north = grid.bbox_wgs84()
    assert west <= -80.80 and south <= -5.35 and east >= -80.55 and north >= -5.10
    assert grid.cell_id(0) == "piura-01-r00c00"
    assert grid.bounds(0)[3] == grid.y_top and grid.bounds(grid.n - 1)[2] == grid.x0 + grid.ncols * 1000

    path = tmp_path / "grid.geojson"
    path.write_text(json.dumps(grid.to_geojson()), encoding="utf-8")
    fc = json.loads(path.read_text(encoding="utf-8"))
    assert all(set(f["properties"]) == {"cell_id"} for f in fc["features"])  # solo cell_id
    back = Grid.from_geojson(path)
    assert (back.aoi_id, back.x0, back.y_top, back.nrows, back.ncols) == \
           (grid.aoi_id, grid.x0, grid.y_top, grid.nrows, grid.ncols)
    idx = back.index(__import__("rasterio").transform.from_origin(grid.x0, grid.y_top, 500, 500),
                     (grid.nrows * 2 + 2, grid.ncols * 2 + 2))
    assert idx[0, 0] == 0 and idx[1, 2] == 1 and idx[2, 0] == grid.ncols      # 2x2 pixeles por celda
    assert (idx[-1, :] == -1).all() and (idx[:, -1] == -1).all()             # fuera de la grilla

    other = Grid.from_bbox("valida-01", [100.30, 14.30, 100.52, 14.52], "EPSG:32647")  # otra zona UTM
    (tmp_path / "other.geojson").write_text(json.dumps(other.to_geojson()), encoding="utf-8")
    again = Grid.from_geojson(tmp_path / "other.geojson")
    assert again.crs == "EPSG:32647" and (again.x0, again.y_top, again.n) == (other.x0, other.y_top, other.n)
    w, s, e, n = again.bbox_wgs84()
    assert w <= 100.30 and e >= 100.52 and s <= 14.30 and n >= 14.52 and back.crs == "EPSG:32717"

    fc["features"][3]["properties"]["cell_id"] = "piura-01-r09c09"           # id que no calza
    path.write_text(json.dumps(fc), encoding="utf-8")
    with pytest.raises(ValueError):
        Grid.from_geojson(path)


@pytest.fixture
def scene(tmp_path):
    out, inputs = tmp_path / "synth-01", tmp_path / "inputs"
    make_scene(out, inputs)
    return out, inputs


def test_end_to_end_synthetic(scene, tmp_path):
    out, inputs = scene
    r = _run("03_change.py", "--in", out)
    assert r.returncode == 0, r.stdout + r.stderr
    bundle = tmp_path / "data" / "synth-01" / "v1"
    r = _run("06_cells.py", "--in", out, "--inputs", inputs, "--bundle", bundle)
    assert r.returncode == 0, r.stdout + r.stderr
    assert "SIMULADA" in r.stdout

    fc = json.loads((bundle / "cells.geojson").read_text(encoding="utf-8"))
    manifest = json.loads((bundle / "manifest.json").read_text(encoding="utf-8"))
    assert contract.errors("cells", fc) == [] and contract.errors("manifest", manifest) == []
    assert manifest["data_origin"] == "simulated" and manifest["opera"]["status"] == "available"
    cells = {f["properties"]["cell_id"]: f["properties"] for f in fc["features"]}
    grid_ids = [f["properties"]["cell_id"] for f in
                json.loads((inputs / "grid.geojson").read_text(encoding="utf-8"))["features"]]
    assert list(cells) == grid_ids and len(cells) == 36      # mismos ids que la grilla

    assert cells["synth-01-r00c00"]["valid_fraction"] == 0    # franja sin dato
    assert cells["synth-01-r00c00"]["p90_abs_delta_db"] is None
    assert cells["synth-01-r00c01"]["valid_fraction"] == pytest.approx(0.5)  # < 0.60
    dec = cells["synth-01-r01c03"]                            # el parche que bajo cubre 40 de 50 filas
    assert dec["decrease_fraction"] == pytest.approx(0.8, abs=0.02) and dec["increase_fraction"] < 0.02
    assert dec["p90_abs_delta_db"] > 6
    assert (dec["opera_status"], dec["opera_overlap_ratio"]) == ("corroborated", 0.95)
    inc = cells["synth-01-r04c02"]                            # dentro del parche que subio
    assert inc["increase_fraction"] > 0.5 and inc["decrease_fraction"] < 0.02
    assert cells["synth-01-r05c05"]["changed_fraction"] < 0.02
    assert cells["synth-01-r05c05"]["opera_overlap_ratio"] is None
    assert cells["synth-01-r02c03"]["road_m_near_change"] == 1000
    for p in cells.values():
        assert p["changed_fraction"] == pytest.approx(p["decrease_fraction"] + p["increase_fraction"])

    nw, ne, se, sw = json.loads((bundle / "overlays" / "bounds.json").read_text())["coordinates"]
    assert nw[0] < ne[0] and nw[1] > sw[1] and ne[1] > se[1]
    sizes = {Image.open(bundle / "overlays" / f"{n}.png").size for n in ("t1", "t2", "delta")}
    assert len(sizes) == 1                                    # las tres capas calzan
    alpha = np.array(Image.open(bundle / "overlays" / "delta.png"))[..., 3]
    assert (alpha == 0).any() and (alpha == 255).any()        # transparente donde no hay dato
    roads = json.loads((bundle / "roads.geojson").read_text(encoding="utf-8"))
    lon, lat = roads["features"][0]["geometry"]["coordinates"][0]
    assert -81 < lon < -80 and -6 < lat < -5                  # vias en EPSG:4326 para el mapa
    changed = np.array(Image.open(out / "changed.tif"))
    assert set(np.unique(changed)) == {0, 1, 255} and changed.shape == (60, 60)   # malla de 100 m
    summary = json.loads((out / "change_summary.json").read_text(encoding="utf-8"))
    assert summary["multilook_meters"] == 100 and summary["expected_false_change_fraction"] < 0.001
    assert cells["synth-01-r05c05"]["changed_fraction"] == 0   # sin ruido marcado como cambio

    # Pixel a pixel (el calculo original del plan) sigue disponible y avisa del ruido
    r = _run("03_change.py", "--in", out, "--multilook-m", 0)
    assert "dominado por ruido" not in r.stdout                # 40 looks: aqui el ruido no domina
    assert np.array(Image.open(out / "changed.tif")).shape == (300, 300)

    r = _run("06_cells.py", "--in", out, "--inputs", inputs, "--bundle", bundle)
    assert r.returncode != 0 and "R28" in r.stdout + r.stderr  # nunca sobrescribe una version


def test_exclusion_mask_from_base_pair(scene):
    out, inputs = scene
    assert _run("03_change.py", "--in", out).returncode == 0           # el par hace de "base"
    mask_path = out / "exclude.tif"
    r = _run("make_exclude.py", "--base", out, "--out", mask_path, "--grow-blocks", 0)
    assert r.returncode == 0, r.stdout + r.stderr
    mask = np.array(Image.open(mask_path))
    assert mask.shape == (60, 60) and mask[12:28, 30:46].all()         # el parche que bajo
    assert mask[40:50, 20:32].all() and not mask[52:, 50:].any()       # el que subio; el resto no
    info = json.loads(mask_path.with_suffix(".json").read_text(encoding="utf-8"))
    assert info["fraction_water"] == 0 and 0.09 < info["fraction_habitual_change"] < 0.11

    # Agua permanente de otra fuente (p. ej. OPERA) en vez del umbral de brillo
    from rasterio.transform import from_origin
    from wombat_nisar import raster_io
    water = np.zeros((200, 200), dtype=np.uint8)
    water[:, 160:] = 1                                                 # franja este: x >= 4800 m
    raster_io.write(out / "agua_externa.tif", water, from_origin(538000, 9428000, 30, 30), "EPSG:32717")
    other = out / "exclude_externa.tif"
    r = _run("make_exclude.py", "--base", out, "--out", other, "--grow-blocks", 0,
             "--water-raster", out / "agua_externa.tif")
    assert r.returncode == 0, r.stdout + r.stderr
    ext = np.array(Image.open(other))
    assert ext[:, 49:].all() and not ext[52:, 16:46].any()             # la franja queda excluida
    ext_info = json.loads(other.with_suffix(".json").read_text(encoding="utf-8"))
    assert "raster externo" in ext_info["water_source"] and 0.19 < ext_info["fraction_water"] < 0.21

    r = _run("03_change.py", "--in", out)                              # ahora la aplica sola
    assert r.returncode == 0 and "Mascara de exclusion" in r.stdout
    assert _run("06_cells.py", "--in", out, "--inputs", inputs).returncode == 0
    cells = {f["properties"]["cell_id"]: f["properties"] for f in
             json.loads((out / "cells.geojson").read_text(encoding="utf-8"))["features"]}
    inc = cells["synth-01-r04c02"]                                     # celda del parche que subio
    assert inc["valid_fraction"] == 0 and inc["changed_fraction"] == 0 and inc["p90_abs_delta_db"] is None
    assert cells["synth-01-r01c03"]["valid_fraction"] == pytest.approx(0.2)   # 80 % excluido
    assert cells["synth-01-r05c05"]["valid_fraction"] == 1
    assert max(c["changed_fraction"] for c in cells.values()) == 0     # nada habitual llega al ranking
    assert _run("03_change.py", "--in", out, "--exclude", out / "no-existe.tif").returncode == 2


def test_opera_contrast(scene, tmp_path):
    from rasterio.transform import from_origin
    from wombat_nisar import raster_io
    out, inputs = scene
    assert _run("03_change.py", "--in", out).returncode == 0
    transform = from_origin(538000, 9428000, 30, 30)                   # malla OPERA de 30 m
    before = np.zeros((200, 200), dtype=np.uint8)
    after = before.copy()
    after[40:93, 100:127] = 1          # agua nueva: x 3000-3810 m, dentro del parche que bajo
    after[150:, :] = 250               # franja enmascarada por HAND: no cuenta
    name = "OPERA_L3_DSWx-S1_T17MNT_{}T110000Z_20261101T000000Z_S1D_30_v1.0_B01_WTR.tif"
    files = {}
    for key, date, data in (("t1", "20261013", before), ("t2", "20261025", after),
                            ("far", "20261105", after)):
        files[key] = tmp_path / name.format(date)
        raster_io.write(files[key], data, transform, "EPSG:32717", nodata=255)

    cells_path = inputs / "opera_cells.json"
    r = _run("validate_opera.py", "--in", out, "--grid", inputs / "grid.geojson",
             "--opera-t1", files["t1"], "--opera-t2", files["t2"], "--out", cells_path)
    assert r.returncode == 0, r.stdout + r.stderr
    doc = json.loads(cells_path.read_text(encoding="utf-8"))
    assert doc["derived_from"] == "Sentinel-1" and doc["t1"]["gap_days"] == 1 and doc["within_max_gap"]
    cell = doc["cells"]["synth-01-r01c03"]                             # x 3000-4000: 81 % con agua
    assert cell["opera_status"] == "corroborated" and 0.75 < cell["opera_overlap_ratio"] < 0.87
    assert doc["cells"]["synth-01-r01c04"]["opera_status"] == "not_corroborated"   # sin agua OPERA
    assert doc["cells"]["synth-01-r04c02"]["opera_overlap_ratio"] == 0  # subio, pero enmascarado/sin agua
    assert doc["stats"]["of_opera_new_water_is_nisar_changed"] > 0.95

    r = _run("06_cells.py", "--in", out, "--inputs", inputs)           # 06 lo consume tal cual
    assert r.returncode == 0, r.stdout + r.stderr
    manifest = json.loads((out / "manifest.json").read_text(encoding="utf-8"))
    assert manifest["opera"]["status"] == "available" and manifest["opera"]["t2"]["gap_days"] == 1
    assert contract.errors("manifest", manifest) == []

    r = _run("validate_opera.py", "--in", out, "--grid", inputs / "grid.geojson",
             "--opera-t1", files["t1"], "--opera-t2", files["far"], "--out", cells_path)
    doc = json.loads(cells_path.read_text(encoding="utf-8"))
    assert "unavailable" in r.stdout and not doc["within_max_gap"]     # 12 dias: fuera de regla
    assert all(c["opera_status"] == "unavailable" for c in doc["cells"].values())


def test_rejects_bad_inputs(scene):
    out, inputs = scene
    assert _run("03_change.py", "--in", out).returncode == 0
    opera = json.loads((inputs / "opera_cells.json").read_text(encoding="utf-8"))
    opera["cells"]["synth-01-r00c02"] = {"opera_status": "available", "opera_overlap_ratio": 0.3}
    (inputs / "opera_cells.json").write_text(json.dumps(opera), encoding="utf-8")
    r = _run("06_cells.py", "--in", out, "--inputs", inputs)
    assert r.returncode == 1 and "NO CUMPLE" in r.stdout      # 'available' no es valido por celda

    opera["cells"] = {"synth-01-r99c99": {"opera_status": "unavailable", "opera_overlap_ratio": None}}
    (inputs / "opera_cells.json").write_text(json.dumps(opera), encoding="utf-8")
    r = _run("06_cells.py", "--in", out, "--inputs", inputs)
    assert r.returncode != 0 and "grid.geojson" in r.stdout + r.stderr


def test_inputs_are_written_as_rust_reads_them(scene):
    out, inputs = scene
    assert _run("03_change.py", "--in", out).returncode == 0
    opera = json.loads((inputs / "opera_cells.json").read_text(encoding="utf-8"))
    opera["cells"]["synth-01-r00c02"] = {"opera_status": "corroborated", "opera_overlap_ratio": 1 / 3}
    (inputs / "opera_cells.json").write_text(json.dumps(opera), encoding="utf-8")
    roads = json.loads((inputs / "road_cells.json").read_text(encoding="utf-8"))
    roads["synth-01-r00c02"] = {"road_m_near_change": 2399.7, "road_m_total": 5100.2}
    (inputs / "road_cells.json").write_text(json.dumps(roads), encoding="utf-8")
    r = _run("06_cells.py", "--in", out, "--inputs", inputs)
    assert r.returncode == 0, r.stdout + r.stderr
    text = (out / "cells.geojson").read_text(encoding="utf-8")
    cell = next(f["properties"] for f in json.loads(text)["features"]
                if f["properties"]["cell_id"] == "synth-01-r00c02")
    assert cell["opera_overlap_ratio"] == 0.3333             # 4 decimales, no 0.3333333333333333
    assert (cell["road_m_near_change"], cell["road_m_total"]) == (2400, 5100)
    assert "e-" not in text and "NaN" not in text            # sin notacion exponencial ni NaN
    for feature in json.loads(text)["features"]:
        props = feature["properties"]
        assert props["aoi_id"] == "synth-01"
        assert props["source_status"]["nisar"] == "simulated", (
            "synthetic input must not be labeled as available real NISAR evidence"
        )
        assert set(props["provenance_refs"]) == {"nisar", "opera", "osm"}
        assert len(props) == 13


def test_roads_are_measured_from_roads_geojson(scene):
    out, inputs = scene
    assert _run("03_change.py", "--in", out).returncode == 0
    (inputs / "road_cells.json").unlink()                              # solo queda roads.geojson
    (inputs / "osm-metadata.json").write_text(json.dumps({
        "query": "way[highway]", "retrieved_at": "2026-11-15T04:59:09Z",
        "attribution": "© OpenStreetMap contributors", "source": "Overpass API"}), encoding="utf-8")
    r = _run("06_cells.py", "--in", out, "--inputs", inputs)
    assert r.returncode == 0 and "Vias:" in r.stdout, r.stdout + r.stderr
    cells = {f["properties"]["cell_id"]: f["properties"] for f in
             json.loads((out / "cells.geojson").read_text(encoding="utf-8"))["features"]}
    road = lambda cid: (cells[cid]["road_m_near_change"], cells[cid]["road_m_total"])
    # Via este-oeste en y = 9425800 (fila r02), de x = 538200 a 543800
    assert road("synth-01-r02c00")[1] == 800 and road("synth-01-r02c01")[1] == 1000
    assert road("synth-01-r02c05") == (0, 800)
    # El parche que bajo va de x = 541000 a 542600; con 250 m de margen: 540750 a 542850
    assert road("synth-01-r02c02") == (250, 1000)
    assert road("synth-01-r02c03") == (1000, 1000)
    assert road("synth-01-r02c04") == (850, 1000)
    # Via norte-sur en x = 543500, lejos de todo cambio
    assert road("synth-01-r03c05") == (0, 700) and road("synth-01-r04c05") == (0, 1000)
    assert road("synth-01-r00c00") == (0, 0)
    manifest = json.loads((out / "manifest.json").read_text(encoding="utf-8"))
    assert manifest["osm"] == {"query": "way[highway]", "retrieved_at": "2026-11-15T04:59:09Z",
                               "attribution": "© OpenStreetMap contributors"}   # sin claves extra


def test_missing_inputs_are_reported_not_invented(scene):
    out, inputs = scene
    assert _run("03_change.py", "--in", out).returncode == 0
    for name in ("roads.geojson", "opera_cells.json", "road_cells.json"):
        (inputs / name).unlink()
    r = _run("06_cells.py", "--in", out, "--inputs", inputs)
    assert r.returncode == 1 and "falta roads.geojson" in r.stdout
    fc = json.loads((out / "cells.geojson").read_text(encoding="utf-8"))
    assert contract.errors("cells", fc) == []                 # borrador valido: OPERA unavailable
    assert all(f["properties"]["opera_status"] == "unavailable" for f in fc["features"])


def _fake_granule(path, *, track=104, frame=175, seed=1, x0=537000.0, y0=9429000.0, n=400):
    """HDF5 con la estructura de la especificacion GCOV (D-102274 Rev F, sec. 5). No es dato NISAR."""
    import h5py
    rng = np.random.default_rng(seed)
    with h5py.File(path, "w") as h5:
        ident = h5.create_group("/science/LSAR/identification")
        ident["trackNumber"] = np.uint32(track)
        ident["frameNumber"] = np.uint16(frame)
        ident["orbitPassDirection"] = "Ascending"
        ident["listOfFrequencies"] = np.array([b"A"])
        g = h5.create_group("/science/LSAR/GCOV/grids/frequencyA")
        g["listOfCovarianceTerms"] = np.array([b"HHHH", b"HVHV"])
        g["xCoordinates"] = x0 + 10 + 20 * np.arange(n)         # centros de pixel
        g["yCoordinates"] = y0 - 10 - 20 * np.arange(n)         # norte-arriba
        g["projection"] = np.uint32(32717)
        g["HHHH"] = rng.gamma(40, 0.08 / 40, (n, n)).astype(np.float32)
        mask = np.ones((n, n), dtype=np.uint8)
        mask[:, :60] = 255                                       # fuera de la adquisicion
        g["mask"] = mask
        g["numberOfLooks"] = np.full((n, n), 40, dtype=np.float32)
    return path


def test_subset_reads_window_and_builds_manifest(tmp_path):
    import h5py
    name = "NISAR_L2_PR_GCOV_0{}_104_A_175_2005_DHDH_A_202610{}T110900_202610{}T110934_P05023_N_F_J_001"
    t1 = _fake_granule(tmp_path / (name.format(32, "07", "07") + ".h5"), seed=1)
    t2 = _fake_granule(tmp_path / (name.format(33, "19", "19") + ".h5"), seed=2)
    inputs, out = tmp_path / "inputs", tmp_path / "out"
    inputs.mkdir()
    grid = Grid("piura-01", 538000, 9428000, 6, 6)
    (inputs / "grid.geojson").write_text(json.dumps(grid.to_geojson()), encoding="utf-8")
    common = ("--aoi", "piura-01", "--aoi-name", "prueba", "--grid", inputs / "grid.geojson",
              "--out", out)

    r = _run("02_subset.py", *common, "--t1", t1, "--t2", t2, "--maturity", "PROVISIONAL")
    assert r.returncode == 0, r.stdout + r.stderr
    manifest = json.loads((out / "manifest.json").read_text(encoding="utf-8"))
    nisar = manifest["nisar"]
    assert nisar["comparable"] is True and nisar["frame"] == "175" and nisar["pixel_spacing_m"] == 20
    assert nisar["t1"]["datetime"] == "2026-10-07T11:09:00Z" and manifest["data_origin"] == "nisar_real"
    import rasterio
    with rasterio.open(out / "t1.tif") as src, h5py.File(t1) as h5:
        assert src.crs.to_epsg() == 32717 and src.shape == (300, 300)         # 6 km a 20 m
        assert (src.transform.c, src.transform.f) == (538000.0, 9428000.0)    # calza con la grilla
        source = h5["/science/LSAR/GCOV/grids/frequencyA/HHHH"][50:350, 50:350]
        assert np.array_equal(src.read(1), source)                            # la ventana correcta
    with rasterio.open(out / "mask_t1.tif") as src:
        assert set(np.unique(src.read(1))) == {1, 255}

    assert _run("03_change.py", "--in", out).returncode == 0                  # encadena con 03 y 06
    r = _run("06_cells.py", "--in", out, "--inputs", inputs)
    cells = json.loads((out / "cells.geojson").read_text(encoding="utf-8"))
    assert contract.errors("cells", cells) == [] and "falta roads.geojson" in r.stdout
    props = {f["properties"]["cell_id"]: f["properties"] for f in cells["features"]}
    assert props["piura-01-r00c00"]["valid_fraction"] == pytest.approx(0.8)   # 10 de 50 columnas son relleno
    assert props["piura-01-r00c03"]["valid_fraction"] == 1

    # Madurez mezclada (R7): manifest con comparable=false y ningun raster
    out2 = tmp_path / "out2"
    (out2).mkdir()
    (out2 / "candidates.json").write_text(json.dumps({"candidates": [
        {"granule": t1.stem, "maturity": "PROVISIONAL"},
        {"granule": t2.stem, "maturity": "VALIDATED"}]}), encoding="utf-8")
    r = _run("02_subset.py", *common[:-1], out2, "--t1", t1, "--t2", t2)
    assert r.returncode == 2 and "NO  same_maturity" in r.stdout
    assert json.loads((out2 / "manifest.json").read_text())["nisar"]["comparable"] is False
    assert not (out2 / "t1.tif").exists()

    # El nombre dice pista 104 y el archivo dice 105: se rechaza
    (tmp_path / "bad").mkdir()
    bad = _fake_granule(tmp_path / "bad" / t2.name, track=105, seed=3)
    r = _run("02_subset.py", *common[:-1], tmp_path / "out3", "--t1", t1, "--t2", bad,
             "--maturity", "PROVISIONAL")
    assert r.returncode != 0 and "segun el archivo" in r.stdout + r.stderr


def test_refuses_non_comparable(scene):
    out, _ = scene
    manifest = json.loads((out / "manifest.json").read_text(encoding="utf-8"))
    manifest["nisar"]["comparable"] = False
    (out / "manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
    r = _run("03_change.py", "--in", out)
    assert r.returncode == 2 and "R6" in r.stdout
    assert not (out / "delta_db.tif").exists()
