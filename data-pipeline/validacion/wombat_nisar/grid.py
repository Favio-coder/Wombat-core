"""Grilla de celdas de 1 km sobre el AOI (Plan Maestro §6.8).

Es el insumo comun: Jelibeth calcula OPERA y vias sobre los mismos cell_id.
Alineada a multiplos de 1000 m en EPSG:32717; fila y columna desde la esquina noroeste.
"""
from __future__ import annotations

import json
import math
import re
from pathlib import Path

import numpy as np
from pyproj import Transformer

CELL_M = 1000
CRS = "EPSG:32717"  # zona del contrato v1; otra zona UTM solo para corridas de validacion
_CRS_URN = "urn:ogc:def:crs:EPSG::"
_CELL_ID = re.compile(r"^(?P<aoi>[a-z0-9]+(-[a-z0-9]+)*)-r(?P<row>\d{2})c(?P<col>\d{2})$")
_AOI_ID = re.compile(r"^[a-z0-9]+(-[a-z0-9]+)*$")


class Grid:
    def __init__(self, aoi_id: str, x0: int, y_top: int, nrows: int, ncols: int,
                 crs: str = CRS):
        if not _AOI_ID.match(aoi_id):
            raise ValueError(f"aoi_id invalido: {aoi_id!r} (minusculas, digitos y guiones)")
        if x0 % CELL_M or y_top % CELL_M:
            raise ValueError("La grilla debe estar alineada a multiplos de 1000 m")
        if not (1 <= nrows <= 100 and 1 <= ncols <= 100):
            raise ValueError(f"Grilla de {nrows}x{ncols}: cell_id rNNcNN admite hasta 100x100")
        self.aoi_id, self.x0, self.y_top = aoi_id, int(x0), int(y_top)
        self.nrows, self.ncols = int(nrows), int(ncols)
        self.n = self.nrows * self.ncols
        self.crs = crs

    @classmethod
    def from_bbox(cls, aoi_id: str, bbox_wgs84, crs: str = CRS) -> "Grid":
        """Grilla minima alineada que cubre el bbox [oeste, sur, este, norte] en grados."""
        west, south, east, north = bbox_wgs84
        to_utm = Transformer.from_crs("EPSG:4326", crs, always_xy=True)
        xs, ys = to_utm.transform([west, east, east, west], [north, north, south, south])
        x0 = math.floor(min(xs) / CELL_M) * CELL_M
        y_top = math.ceil(max(ys) / CELL_M) * CELL_M
        ncols = math.ceil((max(xs) - x0) / CELL_M)
        nrows = math.ceil((y_top - min(ys)) / CELL_M)
        return cls(aoi_id, x0, y_top, nrows, ncols, crs)

    @classmethod
    def from_geojson(cls, path: str | Path) -> "Grid":
        """Lee grid.geojson y comprueba que cada cell_id calza con su posicion."""
        fc = json.loads(Path(path).read_text(encoding="utf-8"))
        crs_name = (fc.get("crs") or {}).get("properties", {}).get("name", "")
        # Se aceptan las dos formas de nombrar el CRS: la URN que escribe to_geojson() y
        # "EPSG:32717", que es la que usa la grilla publicada en data-pipeline/inputs/.
        code = crs_name.rsplit(":", 1)[-1]
        if not (crs_name.startswith(_CRS_URN) or crs_name.startswith("EPSG:")) or not code.isdigit():
            raise ValueError(f"{path}: falta el miembro 'crs' con el codigo EPSG de la grilla")
        crs = "EPSG:" + code
        placed = []
        for feat in fc["features"]:
            m = _CELL_ID.match(feat["properties"]["cell_id"])
            if not m:
                raise ValueError(f"cell_id invalido: {feat['properties']['cell_id']!r}")
            ring = feat["geometry"]["coordinates"][0]
            west, north = min(p[0] for p in ring), max(p[1] for p in ring)
            placed.append((m["aoi"], int(m["row"]), int(m["col"]), west, north))
        aoi, row, col, west, north = placed[0]
        grid = cls(aoi, round(west - col * CELL_M), round(north + row * CELL_M),
                   max(p[1] for p in placed) + 1, max(p[2] for p in placed) + 1, crs)
        if len(placed) != grid.n:
            raise ValueError(f"{path}: {len(placed)} celdas, se esperaban {grid.n}")
        for aoi_i, row, col, west, north in placed:
            if (aoi_i != aoi or abs(west - (grid.x0 + col * CELL_M)) > 0.01
                    or abs(north - (grid.y_top - row * CELL_M)) > 0.01):
                raise ValueError(f"{path}: la celda r{row:02d}c{col:02d} no calza con la grilla")
        return grid

    def cell_id(self, k: int) -> str:
        row, col = divmod(k, self.ncols)
        return f"{self.aoi_id}-r{row:02d}c{col:02d}"

    def ids(self) -> list[str]:
        return [self.cell_id(k) for k in range(self.n)]

    def bounds(self, k: int) -> tuple[int, int, int, int]:
        """(oeste, sur, este, norte) de la celda k en metros EPSG:32717."""
        row, col = divmod(k, self.ncols)
        west = self.x0 + col * CELL_M
        north = self.y_top - row * CELL_M
        return west, north - CELL_M, west + CELL_M, north

    def index(self, transform, shape) -> np.ndarray:
        """Indice de celda de cada pixel de un raster (segun el centro del pixel); -1 = fuera."""
        if transform.b != 0 or transform.d != 0 or transform.e >= 0 or transform.a <= 0:
            raise ValueError("Se espera un raster norte-arriba sin rotacion")
        height, width = shape
        xs = transform.c + (np.arange(width) + 0.5) * transform.a
        ys = transform.f + (np.arange(height) + 0.5) * transform.e
        col = np.floor((xs - self.x0) / CELL_M).astype(np.int64)
        row = np.floor((self.y_top - ys) / CELL_M).astype(np.int64)
        inside = (((row >= 0) & (row < self.nrows))[:, None]
                  & ((col >= 0) & (col < self.ncols))[None, :])
        return np.where(inside, row[:, None] * self.ncols + col[None, :], -1)

    def _ring(self, k: int) -> list[tuple[int, int]]:
        west, south, east, north = self.bounds(k)
        return [(west, north), (east, north), (east, south), (west, south), (west, north)]

    def ring_wgs84(self, k: int) -> list[list[float]]:
        to_wgs84 = Transformer.from_crs(self.crs, "EPSG:4326", always_xy=True)
        return [[round(v, 6) for v in to_wgs84.transform(x, y)] for x, y in self._ring(k)]

    def bbox_wgs84(self) -> list[float]:
        to_wgs84 = Transformer.from_crs(self.crs, "EPSG:4326", always_xy=True)
        east, south = self.x0 + self.ncols * CELL_M, self.y_top - self.nrows * CELL_M
        lons, lats = to_wgs84.transform([self.x0, east, east, self.x0],
                                        [self.y_top, self.y_top, south, south])
        return [round(min(lons), 6), round(min(lats), 6), round(max(lons), 6), round(max(lats), 6)]

    def to_geojson(self) -> dict:
        """grid.geojson: solo cell_id y geometria, en EPSG:32717 (metros enteros)."""
        return {
            "type": "FeatureCollection",
            "crs": {"type": "name", "properties": {"name": _CRS_URN + self.crs.split(":")[1]}},
            "features": [
                {"type": "Feature", "properties": {"cell_id": self.cell_id(k)},
                 "geometry": {"type": "Polygon", "coordinates": [[list(p) for p in self._ring(k)]]}}
                for k in range(self.n)
            ],
        }
