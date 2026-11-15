"""Validacion contra contracts/v1 (los mismos esquemas que usa tools/verify_bundle.py)."""
from __future__ import annotations

import json
import os
from pathlib import Path

from jsonschema import Draft202012Validator, FormatChecker
from referencing import Registry, Resource
from referencing.jsonschema import DRAFT202012


def contracts_dir() -> Path:
    """contracts/v1 del repo, o el que indique WOMBAT_CONTRACTS_DIR (para validar contra otra
    version del contrato sin cambiar de rama)."""
    override = os.environ.get("WOMBAT_CONTRACTS_DIR")
    if override:
        if not (Path(override) / "VERSION").exists():
            raise FileNotFoundError(f"WOMBAT_CONTRACTS_DIR no apunta a un contracts/v1: {override}")
        return Path(override)
    for parent in Path(__file__).resolve().parents:
        if (parent / "contracts" / "v1" / "VERSION").exists():
            return parent / "contracts" / "v1"
    raise FileNotFoundError("No se encontro contracts/v1 (rama sin el contrato v1?)")


def _registry(v1: Path) -> Registry:
    registry = Registry()
    for path in v1.glob("*.schema.json"):
        doc = json.loads(path.read_text(encoding="utf-8"))
        registry = registry.with_resource(
            doc["$id"], Resource.from_contents(doc, default_specification=DRAFT202012))
    return registry


def errors(schema: str, doc: dict) -> list[str]:
    """Errores de `doc` contra <schema>.schema.json ('manifest' o 'cells'); vacio si cumple."""
    v1 = contracts_dir()
    validator = Draft202012Validator(
        json.loads((v1 / f"{schema}.schema.json").read_text(encoding="utf-8")),
        registry=_registry(v1), format_checker=FormatChecker())
    found = []
    for e in sorted(validator.iter_errors(doc), key=lambda e: list(e.absolute_path)):
        where = "/".join(str(p) for p in e.absolute_path) or "(raiz)"
        found.append(f"{where}: {e.message}")
    return found
