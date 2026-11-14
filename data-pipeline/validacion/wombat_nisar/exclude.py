"""Mascara de exclusion (R13): zonas que cambian por razones ajenas al evento.

Se arma a partir de un par BASE (dos escenas sin evento entre ellas):
  - agua permanente: bloques oscuros en las dos fechas (mar, manglar, pozas, rios anchos);
  - cambio habitual: bloques que ya cambian >= umbral sin evento (riego, cultivos, mareas).
Lo excluido se trata como dato invalido: baja valid_fraction de la celda y, si queda bajo el
minimo de calidad, la celda sale como 'insufficient_quality'. No se inventa un campo nuevo.
"""
from __future__ import annotations

import numpy as np
from scipy import ndimage

from . import change

EXCLUDE = 1


def build(g1, g2, valid, transform, delta_base, *, size_m: float, water_db: float,
          change_db: float, grow_blocks: int = 1):
    """Devuelve (mask, water, habitual, transform) en la malla de analisis de `size_m` metros."""
    m1, m2, ok, out_transform, _ = change.multilook(g1, g2, valid, transform, size_m)
    if delta_base.shape != m1.shape:
        raise ValueError("delta_db del par base no esta en la misma malla de analisis")
    with np.errstate(divide="ignore", invalid="ignore"):
        brightest_db = 10.0 * np.log10(np.maximum(m1, m2))
        water = ok & (brightest_db < water_db)
        habitual = np.isfinite(delta_base) & (np.abs(delta_base) >= change_db) & ~water
    mask = water | habitual
    if grow_blocks:
        mask = ndimage.binary_dilation(mask, iterations=grow_blocks)
    return mask, water, habitual, out_transform
