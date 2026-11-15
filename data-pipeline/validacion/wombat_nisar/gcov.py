"""Constantes del producto NISAR L2 GCOV, cada una con su fuente (R11).

Nada de aqui se asume de memoria: lo que la documentacion no confirma queda
marcado como NO CONFIRMADO y se lee del granule real con inspect_granule.py.
"""
from __future__ import annotations

import json
from datetime import datetime, timezone
from pathlib import Path

# Fuente: https://nisar-docs.asf.alaska.edu/gcov/ (leido 2026-11-14)
#   "/science/LSAR/GCOV/grids/frequency[A|B]/HHHH", ".../mask", ".../numberOfLooks"
GRID_GROUP = "/science/LSAR/GCOV/grids/frequency{frequency}"
DS_MASK = "mask"
DS_LOOKS = "numberOfLooks"

# Fuente: https://nisar-docs.asf.alaska.edu/gcov/ (convencion de nombres)
#   NISAR_[I][L]_[PT]_[PROD]_[CYL]_[REL]_[P]_[FRM]_[MODE]_[POLE]_[S]_
#   [StartDateTime]_[EndDateTime]_[CRID]_[A]_[C]_[LOC]_[CTR].[EXT]
# La madurez (BETA/PROVISIONAL/VALIDATED) NO va en el nombre: va en la coleccion.
_FIELDS = (
    "mission", "level", "processing_type", "product", "cycle", "track",
    "orbit_dir", "frame", "mode", "pol_code", "source", "start", "end",
    "crid", "orbit_accuracy", "coverage", "location", "counter",
)
_DIRECTIONS = {"A": "ascending", "D": "descending"}

MASK_RULE = Path(__file__).resolve().parent.parent / "gcov-mask.json"


def parse_granule_name(name: str) -> dict:
    """Separa el nombre de un granule GCOV en sus campos; falla si no sigue la convencion."""
    stem = Path(name).name
    for ext in (".h5", ".hdf5"):
        if stem.endswith(ext):
            stem = stem[: -len(ext)]
    parts = stem.split("_")
    if len(parts) != len(_FIELDS):
        raise ValueError(
            f"Nombre de granule con {len(parts)} campos, se esperaban {len(_FIELDS)}: {name}")
    f = dict(zip(_FIELDS, parts))
    if f["mission"] != "NISAR" or f["product"] != "GCOV":
        raise ValueError(f"No es un granule NISAR GCOV: {name}")
    if f["orbit_dir"] not in _DIRECTIONS:
        raise ValueError(f"Direccion de orbita desconocida '{f['orbit_dir']}': {name}")
    start = datetime.strptime(f["start"], "%Y%m%dT%H%M%S").replace(tzinfo=timezone.utc)
    return {
        "granule": stem,
        "datetime": start.strftime("%Y-%m-%dT%H:%M:%SZ"),
        "cycle": f["cycle"],
        "track": f["track"],
        "frame": f["frame"],
        "orbit_direction": _DIRECTIONS[f["orbit_dir"]],
        "mode": f["mode"],
        "pol_code": f["pol_code"],
    }


def load_mask_rule(path: str | Path = MASK_RULE) -> dict:
    """Lee la regla de decodificacion de la mascara (gcov-mask.json)."""
    rule = json.loads(Path(path).read_text(encoding="utf-8"))
    if not rule.get("valid_values"):
        raise ValueError(f"{path}: falta 'valid_values'")
    return rule
