"""Escena SINTETICA para probar el pipeline sin datos reales.

Nada de esto es dato NISAR: el manifest lleva data_origin = "simulated" y los PNG
salen con la marca SIMULATED DATA (R1).
"""
from __future__ import annotations

import json
from pathlib import Path

import numpy as np
from rasterio.transform import from_origin

from . import raster_io
from .grid import Grid
from .manifest import build_manifest

PIXEL_M = 20
SIZE = 300                       # 6 km x 6 km
LEFT, TOP = 538000, 9428000      # UTM 17S
LOOKS = 40
INVALID_COLS = 75                # franja oeste sin dato: deja una columna de celdas < 0.60
DECREASE = (slice(60, 140), slice(150, 230))   # x0.2  (~ -7 dB)
INCREASE = (slice(200, 250), slice(100, 160))  # x3    (~ +4.8 dB)


def make_scene(out_dir: str | Path, inputs_dir: str | Path, seed: int = 7) -> Grid:
    """Escribe los rasteres y el manifest en out_dir, y la grilla y los insumos en inputs_dir."""
    out, inputs = Path(out_dir), Path(inputs_dir)
    out.mkdir(parents=True, exist_ok=True)
    inputs.mkdir(parents=True, exist_ok=True)
    rng = np.random.default_rng(seed)
    transform = from_origin(LEFT, TOP, PIXEL_M, PIXEL_M)
    crs = raster_io.CRS_COMPUTE
    shape = (SIZE, SIZE)

    speckle = lambda: rng.gamma(LOOKS, 1.0 / LOOKS, shape)
    g1 = (0.08 * speckle()).astype(np.float32)
    truth = np.full(shape, 0.08)
    truth[DECREASE] *= 0.2
    truth[INCREASE] *= 3.0
    g2 = (truth * speckle()).astype(np.float32)
    mask = np.ones(shape, dtype=np.uint8)
    mask[:, :INVALID_COLS] = 0
    g1[:, :INVALID_COLS] = np.nan
    g2[:, :INVALID_COLS] = np.nan
    looks = np.full(shape, LOOKS, dtype=np.float32)

    raster_io.write(out / "t1.tif", g1, transform, crs, nodata=np.nan)
    raster_io.write(out / "t2.tif", g2, transform, crs, nodata=np.nan)
    for t in ("t1", "t2"):
        raster_io.write(out / f"mask_{t}.tif", mask, transform, crs)
        raster_io.write(out / f"looks_{t}.tif", looks, transform, crs)

    grid = Grid(out.name, LEFT, TOP, SIZE * PIXEL_M // 1000, SIZE * PIXEL_M // 1000)
    (inputs / "grid.geojson").write_text(json.dumps(grid.to_geojson()), encoding="utf-8")

    # Insumos de Jelibeth, sinteticos: una via cruza el parche que bajo; agua nueva en su mitad oeste
    water = np.zeros((200, 200), dtype=np.uint8)
    water[40:93, 100:127] = 1
    raster_io.write(inputs / "water_new.tif", water, from_origin(LEFT, TOP, 30, 30), crs, nodata=255)
    roads = {
        "type": "FeatureCollection",
        "crs": {"type": "name", "properties": {"name": "urn:ogc:def:crs:EPSG::32717"}},
        "retrieved_at": "2026-11-14T00:00:00Z", "query": "highway=*",
        "attribution": "© OpenStreetMap contributors",
        "features": [
            {"type": "Feature", "properties": {"highway": "primary"},
             "geometry": {"type": "LineString",
                          "coordinates": [[538200, 9425800], [543800, 9425800]]}},
            {"type": "Feature", "properties": {"highway": "tertiary"},
             "geometry": {"type": "LineString",
                          "coordinates": [[543500, 9422200], [543500, 9424700]]}},
        ],
    }
    (inputs / "roads.geojson").write_text(json.dumps(roads), encoding="utf-8")
    opera = {cid: {"opera_status": "not_corroborated", "opera_overlap_ratio": 0.0}
             for cid in grid.ids()}
    opera[f"{out.name}-r01c03"] = {"opera_status": "corroborated", "opera_overlap_ratio": 0.95}
    opera[f"{out.name}-r05c05"] = {"opera_status": "unavailable", "opera_overlap_ratio": None}
    (inputs / "opera_cells.json").write_text(json.dumps({
        "derived_from": "Sentinel-1",
        "t1": {"granules": ["SIMULATED-OPERA-T1"], "gap_days": 0},
        "t2": {"granules": ["SIMULATED-OPERA-T2"], "gap_days": 0},
        "cells": opera}), encoding="utf-8")
    road_cells = {cid: {"road_m_near_change": 0, "road_m_total": 0} for cid in grid.ids()}
    for col in range(6):
        road_cells[f"{out.name}-r02c{col:02d}"] = {
            "road_m_near_change": 1000 if col in (3, 4) else 0,
            "road_m_total": 800 if col in (0, 5) else 1000}
    (inputs / "road_cells.json").write_text(json.dumps(road_cells), encoding="utf-8")

    scene = dict(track="000", frame="SIMULATED", orbit_direction="ascending", mode="SIMULATED",
                 pol_code="XXXX", maturity="PROVISIONAL", frequency="A", polarization="HHHH")
    manifest = build_manifest(
        aoi_id=out.name, aoi_name="SIMULATED - escena sintetica (no es dato NISAR)",
        bbox_wgs84=grid.bbox_wgs84(), data_origin="simulated", pipeline_commit="synthetic",
        t1=dict(scene, granule="SIMULATED-T1", datetime="2026-10-12T00:00:00Z"),
        t2=dict(scene, granule="SIMULATED-T2", datetime="2026-10-24T00:00:00Z"),
        pixel_spacing_m=PIXEL_M)
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2, ensure_ascii=False),
                                       encoding="utf-8")
    return grid
