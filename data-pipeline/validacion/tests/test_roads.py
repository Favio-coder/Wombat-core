"""Pruebas de 05_roads.py con geometria sintetica de resultado calculado a mano.

Correr:  python -m pytest data-pipeline/validacion -q
"""
import importlib.util
import json
import subprocess
import sys
from pathlib import Path

import numpy as np
import pytest
from pyproj import Transformer
from rasterio.transform import from_origin

from wombat_nisar import assemble, raster_io
from wombat_nisar.grid import Grid
from wombat_nisar.synthetic import make_scene

NISAR = Path(__file__).resolve().parent.parent
CONFIG = NISAR.parents[1] / "config" / "risk-config.json"

# El nombre del script empieza con un digito, asi que no se puede importar por `import`.
_spec = importlib.util.spec_from_file_location("roads05", NISAR / "05_roads.py")
roads05 = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(roads05)


def _run(*args):
    return subprocess.run([sys.executable, str(NISAR / "05_roads.py"), *map(str, args)],
                          capture_output=True, text=True, cwd=NISAR)


# ---------------------------------------------------------------------------
# Caso de papel y lapiz
#
# Grilla de 1 fila x 2 columnas en EPSG:32717, celdas de 1 km:
#   c00: x 500000..501000     c01: x 501000..502000     ambas y 8999000..9000000
#
# Una via recta de 1000 m a y = 8999500, de x = 500700 a x = 501700. El borde
# entre celdas esta en x = 501000, asi que son 300 m en c00 y 700 m en c01.
#
# Un unico pixel con cambio de 10 m en x 501200..501210, y 8999490..8999500. La
# via corre por su borde superior, a distancia 0. Con road_buffer_meters = 250 la
# region dilatada alcanza, a esa altura, x 500950..501460: son 510 m de via,
# 50 m en c00 (500950..501000) y 460 m en c01 (501000..501460).
# ---------------------------------------------------------------------------
X0, Y_TOP = 500000, 9000000
ROAD_Y, ROAD_X1, ROAD_X2 = 8999500, 500700, 501700
CHANGED_ROW, CHANGED_COL = 50, 120
BUFFER_M = 250


@pytest.fixture
def paper(tmp_path):
    """grid.geojson, roads.geojson (en grados) y changed.tif del caso de arriba."""
    grid = Grid("papel-01", X0, Y_TOP, 1, 2)
    (tmp_path / "grid.geojson").write_text(json.dumps(grid.to_geojson()), encoding="utf-8")

    # Las vias se escriben en grados, como las reales de OSM: asi la prueba pasa
    # tambien por la reproyeccion y no solo por el recorte.
    to_wgs84 = Transformer.from_crs(grid.crs, "EPSG:4326", always_xy=True)
    coords = [list(to_wgs84.transform(x, ROAD_Y)) for x in (ROAD_X1, ROAD_X2)]
    (tmp_path / "roads.geojson").write_text(json.dumps({
        "type": "FeatureCollection",
        "features": [{"type": "Feature", "properties": {"highway": "primary"},
                      "geometry": {"type": "LineString", "coordinates": coords}}]}),
        encoding="utf-8")

    changed = np.zeros((100, 200), dtype=np.uint8)
    changed[CHANGED_ROW, CHANGED_COL] = 1
    raster_io.write(tmp_path / "changed.tif", changed,
                    from_origin(X0, Y_TOP, 10, 10), grid.crs)

    (tmp_path / "risk-config.json").write_text(
        json.dumps({"road_buffer_meters": BUFFER_M}), encoding="utf-8")
    return tmp_path, grid


def _cells(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))["cells"]


def test_paper_case_splits_road_and_measures_the_buffer(paper):
    d, _ = paper
    r = _run("--inputs", d, "--changed", d / "changed.tif", "--config", d / "risk-config.json")
    assert r.returncode == 0, r.stdout + r.stderr
    cells = _cells(d / "road_cells.json")
    assert cells["papel-01-r00c00"] == {"road_m_near_change": 50, "road_m_total": 300}
    assert cells["papel-01-r00c01"] == {"road_m_near_change": 460, "road_m_total": 700}
    assert sum(c["road_m_total"] for c in cells.values()) == 1000       # la via entera
    assert sum(c["road_m_near_change"] for c in cells.values()) == 510


def test_metadata_says_how_it_was_measured(paper):
    d, grid = paper
    assert _run("--inputs", d, "--changed", d / "changed.tif",
                "--config", d / "risk-config.json").returncode == 0
    doc = json.loads((d / "road_cells.json").read_text(encoding="utf-8"))
    assert doc["road_buffer_meters"] == BUFFER_M
    assert doc["crs_compute"] == grid.crs
    assert doc["changed"] == "changed.tif"
    assert "warning" not in doc


def test_two_runs_give_identical_bytes(paper):
    d, _ = paper
    args = ("--inputs", d, "--changed", d / "changed.tif", "--config", d / "risk-config.json")
    assert _run(*args).returncode == 0
    first = (d / "road_cells.json").read_bytes()
    assert _run(*args).returncode == 0
    assert (d / "road_cells.json").read_bytes() == first


def test_missing_changed_tif_fails_instead_of_writing_zeros(paper):
    d, _ = paper
    (d / "changed.tif").unlink()
    r = _run("--inputs", d, "--changed", d / "changed.tif", "--config", d / "risk-config.json")
    assert r.returncode == 2
    assert "changed.tif" in r.stdout and "03_change.py" in r.stdout
    assert not (d / "road_cells.json").exists()

    r = _run("--inputs", d, "--config", d / "risk-config.json")       # sin --in ni --changed
    assert r.returncode == 2 and "03_change.py" in r.stdout
    assert not (d / "road_cells.json").exists()


def test_solo_total_is_explicit_and_marked(paper):
    d, _ = paper
    (d / "changed.tif").unlink()
    r = _run("--inputs", d, "--solo-total", "--config", d / "risk-config.json")
    assert r.returncode == 0, r.stdout + r.stderr
    assert "AVISO" in r.stdout
    doc = json.loads((d / "road_cells.json").read_text(encoding="utf-8"))
    assert doc["changed"] is None and "warning" in doc
    cells = doc["cells"]
    assert cells["papel-01-r00c00"] == {"road_m_near_change": 0, "road_m_total": 300}
    assert cells["papel-01-r00c01"] == {"road_m_near_change": 0, "road_m_total": 700}


def test_no_change_near_the_roads_is_said_not_assumed(paper):
    d, grid = paper
    far = np.zeros((100, 200), dtype=np.uint8)
    far[0, 199] = 1                                   # esquina noreste, a >250 m de la via
    raster_io.write(d / "changed.tif", far, from_origin(X0, Y_TOP, 10, 10), grid.crs)
    r = _run("--inputs", d, "--changed", d / "changed.tif", "--config", d / "risk-config.json")
    assert r.returncode == 0, r.stdout + r.stderr
    assert "AVISO" in r.stdout
    cells = _cells(d / "road_cells.json")
    assert all(c["road_m_near_change"] == 0 for c in cells.values())
    assert sum(c["road_m_total"] for c in cells.values()) == 1000      # las vias siguen ahi


def test_rounding_is_half_up_not_bankers():
    assert [roads05._meters(v) for v in (0.5, 1.5, 2.5, 0.4999, 1000.5)] == [1, 2, 3, 0, 1001]


# ---------------------------------------------------------------------------
# Escena sintetica completa: 6x6 celdas de 1 km, dos vias de longitud conocida.
#
#   primaria  y = 9425800, x 538200..543800  -> fila r02, 800 m en c00 y c05 y
#                                               1000 m en c01..c04 (5600 m)
#   terciaria x = 543500, y 9422200..9424700 -> columna c05, 700 m en r03,
#                                               1000 m en r04 y 800 m en r05 (2500 m)
# ---------------------------------------------------------------------------
PRIMARY = {f"synth-01-r02c{c:02d}": 800 if c in (0, 5) else 1000 for c in range(6)}
TERTIARY = {"synth-01-r03c05": 700, "synth-01-r04c05": 1000, "synth-01-r05c05": 800}


@pytest.fixture
def scene(tmp_path):
    out, inputs = tmp_path / "synth-01", tmp_path / "inputs"
    make_scene(out, inputs)
    assert subprocess.run([sys.executable, str(NISAR / "03_change.py"), "--in", str(out)],
                          capture_output=True, text=True, cwd=NISAR).returncode == 0
    return out, inputs


def test_totals_on_the_synthetic_scene_match_hand_numbers(scene):
    out, inputs = scene
    r = _run("--in", out, "--inputs", inputs)
    assert r.returncode == 0, r.stdout + r.stderr
    cells = _cells(inputs / "road_cells.json")
    expected = {**PRIMARY, **TERTIARY}
    assert {cid: c["road_m_total"] for cid, c in cells.items() if c["road_m_total"]} == expected
    assert len(cells) == 36


def test_near_change_is_bounded_by_total_and_is_not_empty(scene):
    out, inputs = scene
    assert _run("--in", out, "--inputs", inputs).returncode == 0
    cells = _cells(inputs / "road_cells.json")
    for cid, c in cells.items():
        assert 0 <= c["road_m_near_change"] <= c["road_m_total"], cid
    # El parche que bajo 7 dB (x 541000..542600) cruza la primaria en r02c03 y r02c04.
    assert cells["synth-01-r02c03"]["road_m_near_change"] > 0
    assert cells["synth-01-r02c04"]["road_m_near_change"] > 0
    # La terciaria esta lejos de los dos parches: nada de exposicion inventada.
    assert cells["synth-01-r04c05"]["road_m_near_change"] == 0


def test_sum_of_totals_matches_the_roads_clipped_to_the_grid(scene):
    """La suma de las celdas = la via recortada a la grilla, medida por separado.

    No tiene por que coincidir con `roads_total_m` que calcula Rust sobre
    roads.geojson: alli entran tambien los tramos fuera de la grilla, y la
    longitud sale de una aproximacion plana en grados, no de la proyeccion UTM.
    """
    out, inputs = scene
    assert _run("--in", out, "--inputs", inputs).returncode == 0
    cells = _cells(inputs / "road_cells.json")
    grid = Grid.from_geojson(inputs / "grid.geojson")

    from shapely.geometry import box
    from shapely.ops import unary_union
    lines = roads05.read_roads(inputs / "roads.geojson", grid)
    envelope = box(grid.x0, grid.y_top - grid.nrows * 1000,
                   grid.x0 + grid.ncols * 1000, grid.y_top)
    measured = unary_union(lines).intersection(envelope).length

    total = sum(c["road_m_total"] for c in cells.values())
    with_roads = sum(1 for c in cells.values() if c["road_m_total"])
    # Cada celda redondea a lo sumo medio metro; nada mas puede separarlas.
    assert abs(total - measured) <= with_roads * 0.5 + 1e-6
    assert total == 8100                                   # 5600 de la primaria + 2500 de la otra


def test_the_output_is_what_assemble_reads(scene):
    """La prueba que importa: 06_cells.py tiene que poder leer este archivo."""
    out, inputs = scene
    assert _run("--in", out, "--inputs", inputs).returncode == 0
    grid = Grid.from_geojson(inputs / "grid.geojson")
    read = assemble.Inputs(inputs, grid)
    assert read.roads is not None, "assemble.Inputs no reconocio el road_cells.json"
    assert set(read.roads) == set(grid.ids())
    cells = _cells(inputs / "road_cells.json")
    for cid, expected in PRIMARY.items():
        assert assemble._meters(read.roads[cid], "road_m_total") == expected
        assert read.roads[cid] == cells[cid]


def test_06_cells_accepts_the_generated_file(scene):
    out, inputs = scene
    assert _run("--in", out, "--inputs", inputs).returncode == 0
    r = subprocess.run([sys.executable, str(NISAR / "06_cells.py"), "--in", str(out),
                        "--inputs", str(inputs)], capture_output=True, text=True, cwd=NISAR)
    assert r.returncode == 0, r.stdout + r.stderr
    assert "sin road_cells.json" not in r.stdout
    fc = json.loads((out / "cells.geojson").read_text(encoding="utf-8"))
    props = {f["properties"]["cell_id"]: f["properties"] for f in fc["features"]}
    for cid, expected in {**PRIMARY, **TERTIARY}.items():
        assert props[cid]["road_m_total"] == expected
    assert all(isinstance(p["road_m_near_change"], int) for p in props.values())
