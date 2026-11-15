"""Quality gate y cambio con signo (Plan Maestro §6.4 y §6.5)."""
from __future__ import annotations

import numpy as np

EPS = 1e-10
CHANGED_NODATA = 255  # changed.tif: 1 = cambio, 0 = sin cambio, 255 = pixel invalido


def quality_gate(g1, g2, mask1, mask2, looks1, looks2, mask_valid_values) -> np.ndarray:
    """valid(p) de §6.4: mascara valida en ambas fechas, looks > 0, gamma0 finito y > 0."""
    with np.errstate(invalid="ignore"):
        ok_mask = np.isin(mask1, mask_valid_values) & np.isin(mask2, mask_valid_values)
        ok_looks = (looks1 > 0) & (looks2 > 0)
        ok_gamma = np.isfinite(g1) & np.isfinite(g2) & (g1 > 0) & (g2 > 0)
    return ok_mask & ok_looks & ok_gamma


def delta_db(g1, g2, valid) -> np.ndarray:
    """delta_db = 10*log10((g2+eps)/(g1+eps)), CON signo (R8). NaN donde no es valido."""
    out = np.full(g1.shape, np.nan, dtype=np.float32)
    out[valid] = 10.0 * np.log10((g2[valid].astype(np.float64) + EPS)
                                 / (g1[valid].astype(np.float64) + EPS))
    return out


def multilook(g1, g2, valid, transform, size_m: float, min_valid: float = 0.5):
    """Promedia gamma0 (potencia lineal) de t1 y t2 en bloques de `size_m` metros.

    Ambas fechas se promedian sobre los mismos pixeles validos. Un bloque es valido si al
    menos `min_valid` de sus pixeles lo es. Los bloques se alinean a multiplos de `size_m`,
    asi calzan con la grilla de 1 km. Devuelve (m1, m2, valid_m, transform_m, k).
    """
    px = transform.a
    k = int(round(size_m / px))
    if k < 1 or abs(k * px - size_m) > 1e-6 or abs(transform.e + px) > 1e-6:
        raise ValueError(f"{size_m} m no es multiplo entero del pixel de {px} m")
    if k == 1:
        return g1, g2, valid, transform, 1
    left = int(round((transform.c - np.floor(transform.c / size_m) * size_m) / px))
    top = int(round((np.ceil(transform.f / size_m) * size_m - transform.f) / px))
    height, width = g1.shape
    pad = ((top, -(top + height) % k), (left, -(left + width) % k))
    ok = np.pad(valid, pad, constant_values=False)
    shape = (ok.shape[0] // k, k, ok.shape[1] // k, k)
    count = ok.reshape(shape).sum(axis=(1, 3))

    def mean(gamma):
        total = np.where(ok, np.pad(gamma, pad, constant_values=0), 0).astype(np.float64)
        total = total.reshape(shape).sum(axis=(1, 3))
        return np.divide(total, count, out=np.full(count.shape, np.nan), where=count > 0)

    valid_m = count >= min_valid * k * k
    out_transform = type(transform)(size_m, 0.0, transform.c - left * px,
                                    0.0, -size_m, transform.f + top * px)
    return mean(g1).astype(np.float32), mean(g2).astype(np.float32), valid_m, out_transform, k


def speckle_std_db(looks: float) -> float:
    """Desviacion de delta_db debida solo al speckle, con `looks` looks independientes por fecha
    (aproximacion para looks altos: 4.343 * sqrt(2 / looks))."""
    return 4.343 * float(np.sqrt(2.0 / looks))


def false_change_fraction(threshold_db: float, noise_std_db: float) -> float:
    """Fraccion de pixeles SIN cambio real que igual cruzarian +-umbral por puro ruido."""
    from math import erfc, sqrt
    return erfc(threshold_db / (noise_std_db * sqrt(2.0)))


def classify(delta, threshold_db: float):
    """Devuelve (decrease, increase, changed). NaN nunca cuenta como cambio."""
    with np.errstate(invalid="ignore"):
        decrease = delta <= -threshold_db
        increase = delta >= threshold_db
    return decrease, increase, decrease | increase


def changed_mask(delta, threshold_db: float) -> np.ndarray:
    """Mascara de cambio para Jelibeth (vias y OPERA): uint8 con 255 donde no hay dato."""
    out = np.full(delta.shape, CHANGED_NODATA, dtype=np.uint8)
    valid = np.isfinite(delta)
    out[valid] = classify(delta, threshold_db)[2][valid]
    return out


def summary(delta, threshold_db: float) -> dict:
    """Resumen para el criterio de hecho de 03: histograma razonable, no todo 0 ni todo NaN."""
    valid = np.isfinite(delta)
    n_valid = int(valid.sum())
    out = {"pixels": int(delta.size), "valid_fraction": n_valid / delta.size, "warnings": []}
    if n_valid == 0:
        out["warnings"].append("TODO NaN: ningun pixel paso el quality gate")
        return out
    v = delta[valid]
    decrease, increase, _ = classify(v, threshold_db)
    p02, p50, p98 = (float(x) for x in np.percentile(v, [2, 50, 98]))
    out.update(
        decrease_fraction=float(decrease.mean()),
        increase_fraction=float(increase.mean()),
        mean_db=float(v.mean()), std_db=float(v.std()),
        p02_db=p02, p50_db=p50, p98_db=p98,
    )
    if out["std_db"] < 0.05:
        out["warnings"].append("TODO ~0: t1 y t2 parecen la misma escena")
    if out["valid_fraction"] < 0.5:
        out["warnings"].append("menos de la mitad de los pixeles es valida")
    if abs(out["p50_db"]) > 1.5:
        out["warnings"].append("mediana lejos de 0 dB: posible diferencia de calibracion o de modo")
    if out["decrease_fraction"] + out["increase_fraction"] < 0.005:
        out["warnings"].append("casi sin cambio (riesgo 2 del plan: contar como linea base)")
    return out
