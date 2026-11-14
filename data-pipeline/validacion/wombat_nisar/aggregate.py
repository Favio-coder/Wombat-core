"""Agregados NISAR por celda (Plan Maestro §6.8, campos de contracts/v1/cells.schema.json)."""
from __future__ import annotations

import numpy as np

from .change import classify
from .grid import CELL_M, Grid


def _count(idx, selector, n) -> np.ndarray:
    chosen = idx[selector]
    return np.bincount(chosen[chosen >= 0], minlength=n)


def _p90_abs(delta, idx, valid, n) -> np.ndarray:
    keep = valid & (idx >= 0)
    cell, values = idx[keep], np.abs(delta[keep])
    order = np.argsort(cell, kind="stable")
    cell, values = cell[order], values[order]
    edges = np.searchsorted(cell, np.arange(n + 1))
    out = np.full(n, np.nan)
    for k in range(n):
        if edges[k + 1] > edges[k]:
            out[k] = np.percentile(values[edges[k]:edges[k + 1]], 90)
    return out


def nisar_metrics(delta, transform, grid: Grid, threshold_db: float) -> dict[str, dict]:
    """Por cell_id: valid_fraction, decrease/increase/changed_fraction, p90_abs_delta_db.

    delta: delta_db con NaN = invalido. valid_fraction se mide contra el area completa de
    la celda, asi que una celda sin cobertura del raster queda en 0. Las fracciones de
    cambio son sobre pixeles validos. Sin NaN en la salida: p90 es None si no hay validos.
    """
    idx = grid.index(transform, delta.shape)
    valid = np.isfinite(delta)
    decrease, increase, _ = classify(delta, threshold_db)
    n_valid = _count(idx, valid, grid.n)
    n_decrease = _count(idx, decrease, grid.n)
    n_increase = _count(idx, increase, grid.n)
    expected = (CELL_M / transform.a) * (CELL_M / -transform.e)
    p90 = _p90_abs(delta, idx, valid, grid.n)

    out = {}
    for k in range(grid.n):
        nv = int(n_valid[k])
        dec = round(int(n_decrease[k]) / nv, 4) if nv else 0.0
        inc = round(int(n_increase[k]) / nv, 4) if nv else 0.0
        out[grid.cell_id(k)] = {
            "valid_fraction": round(min(nv / expected, 1.0), 4),
            "decrease_fraction": dec,
            "increase_fraction": inc,
            "changed_fraction": round(dec + inc, 4),
            "p90_abs_delta_db": round(float(p90[k]), 2) if nv else None,
        }
    return out
