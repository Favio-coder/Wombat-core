"""Exposicion vial por celda (Plan Maestro §6.7).

change_zone = zona con cambio, ampliada `buffer_m` metros.
road_m_total(celda)       = longitud de vias dentro de la celda.
road_m_near_change(celda) = longitud de vias dentro de la celda y de change_zone.
Las longitudes se miden en la zona UTM de la grilla, en metros.
"""
from __future__ import annotations

import math

import geopandas as gpd
import numpy as np
import shapely
from rasterio import features
from shapely.geometry import box, shape
from shapely.ops import unary_union

from .grid import Grid

CHANGED = 1  # changed.tif: 1 = cambio, 0 = sin cambio, 255 = invalido


def load_roads(path, crs: str):
    """Lee roads.geojson (WGS84 si no declara CRS) y lo lleva al CRS de la grilla."""
    gdf = gpd.read_file(path)
    if gdf.crs is None:
        gdf = gdf.set_crs("EPSG:4326")
    gdf = gdf.to_crs(crs)
    lines = [g for g in gdf.geometry if g is not None and not g.is_empty
             and g.geom_type in ("LineString", "MultiLineString")]
    return unary_union(lines) if lines else None


def road_cells(roads, changed, transform, buffer_m: float, grid: Grid) -> dict[str, dict]:
    """Por cell_id: road_m_near_change y road_m_total, en metros enteros (redondeo half-up)."""
    out = {cid: {"road_m_near_change": 0, "road_m_total": 0} for cid in grid.ids()}
    if roads is None:
        return out
    boxes = np.array([box(*grid.bounds(k)) for k in range(grid.n)])
    in_cell = shapely.intersection(roads, boxes)
    total = shapely.length(in_cell)
    is_changed = changed == CHANGED
    near = np.zeros(grid.n)
    if is_changed.any():
        polygons = [shape(geom) for geom, _ in
                    features.shapes(is_changed.astype(np.uint8), mask=is_changed, transform=transform)]
        zone = unary_union(polygons).buffer(buffer_m)
        near = shapely.length(shapely.intersection(in_cell, zone))
    for k, cid in enumerate(grid.ids()):
        out[cid] = {"road_m_near_change": int(math.floor(near[k] + 0.5)),
                    "road_m_total": int(math.floor(total[k] + 0.5))}
    return out
