"""Comparabilidad t1/t2 (R6, R7) y manifest.json segun contracts/v1/manifest.schema.json."""
from __future__ import annotations

from .gcov import parse_granule_name

CHECKS = ("same_frame", "same_orbit_direction", "same_mode",
          "same_frequency", "same_polarization", "same_maturity")
OPERA_UNAVAILABLE = {
    "product": "OPERA_L3_DSWx-S1", "derived_from": "Sentinel-1", "status": "unavailable",
    "t1": {"granules": [], "gap_days": None}, "t2": {"granules": [], "gap_days": None},
}


def scene_from_granule(name: str, maturity: str, frequency: str = "A",
                       polarization: str = "HHHH") -> dict:
    """Describe una escena. `maturity` viene de la coleccion donde se encontro el granule."""
    scene = parse_granule_name(name)
    scene.update(maturity=maturity.upper(), frequency=frequency, polarization=polarization)
    return scene


def check_comparable(t1: dict, t2: dict) -> tuple[bool, dict]:
    """Devuelve (comparable, comparable_checks). Si un chequeo falla, no se procesa (R6)."""
    checks = {
        "same_frame": t1["frame"] == t2["frame"] and t1["track"] == t2["track"],
        "same_orbit_direction": t1["orbit_direction"] == t2["orbit_direction"],
        "same_mode": t1["mode"] == t2["mode"],
        "same_frequency": t1["frequency"] == t2["frequency"],
        "same_polarization": (t1["polarization"] == t2["polarization"]
                              and t1["pol_code"] == t2["pol_code"]),
        "same_maturity": bool(t1["maturity"]) and t1["maturity"] == t2["maturity"],
    }
    return all(checks.values()), checks


def build_manifest(*, aoi_id: str, aoi_name: str, bbox_wgs84: list[float],
                   t1: dict, t2: dict, pixel_spacing_m: int, data_origin: str = "nisar_real",
                   pipeline_commit: str = "unknown",
                   risk_config_version: str = "risk-config/0.1",
                   crs_compute: str = "EPSG:32717") -> dict:
    """Parte NISAR del manifest. `opera` y `osm` los completa 06 con los insumos de Jelibeth."""
    if data_origin not in ("nisar_real", "simulated"):
        raise ValueError("data_origin debe ser nisar_real o simulated (R1)")
    if t1["datetime"] >= t2["datetime"]:
        raise ValueError("t1 debe ser anterior a t2")
    comparable, checks = check_comparable(t1, t2)
    return {
        "aoi_id": aoi_id,
        "aoi_name": aoi_name,
        "data_origin": data_origin,
        "crs_compute": crs_compute,  # el contrato v1 solo admite EPSG:32717
        "bbox_wgs84": [round(float(v), 6) for v in bbox_wgs84],
        "nisar": {
            "product": "NISAR_L2_GCOV",
            "maturity": t1["maturity"],
            "frame": t1["frame"],
            "orbit_direction": t1["orbit_direction"],
            "mode": t1["mode"],
            "frequency": t1["frequency"],
            "polarization": t1["polarization"],
            "pixel_spacing_m": int(pixel_spacing_m),
            "t1": {"granule": t1["granule"], "datetime": t1["datetime"]},
            "t2": {"granule": t2["granule"], "datetime": t2["datetime"]},
            "comparable": comparable,
            "comparable_checks": checks,
        },
        "opera": dict(OPERA_UNAVAILABLE),
        "pipeline": {"commit": pipeline_commit, "risk_config_version": risk_config_version},
    }
