"""Valida que un paquete local de evidencia y sus bloques pertenezcan a un solo AOI."""
from __future__ import annotations

import argparse
import json
import math
import re
import sys
from pathlib import Path
from typing import Any

PURPOSES = {"scientific", "baseline", "preparation"}
KNOWN_AOIS = {
    "piura-01",
    "piura-demo",
    "sechura-01",
    "tailandia-01",
    "tumbes-01",
    "huaraz-01",
    "jequetepeque-01",
}
AOI_CRS = {
    "piura-01": "EPSG:32717",
    "piura-demo": "EPSG:32717",
    "sechura-01": "EPSG:32717",
    "tailandia-01": "EPSG:32647",
    "tumbes-01": "EPSG:32717",
    "huaraz-01": "EPSG:32718",
    "jequetepeque-01": "EPSG:32717",
}
COMPARABLE_CHECKS = (
    "same_frame",
    "same_orbit_direction",
    "same_mode",
    "same_frequency",
    "same_polarization",
    "same_maturity",
)


def _read_json(path: Path, errors: list[str]) -> Any | None:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        errors.append(f"falta {path}")
    except (OSError, json.JSONDecodeError) as exc:
        errors.append(f"{path}: {exc}")
    return None


def _bbox_intersects(left: list[float], right: list[float]) -> bool:
    return left[0] <= right[2] and left[2] >= right[0] and left[1] <= right[3] and left[3] >= right[1]


def _geometry_bbox(geometry: Any) -> list[float] | None:
    points: list[tuple[float, float]] = []

    def visit(value: Any) -> None:
        if (isinstance(value, list) and len(value) >= 2
                and all(isinstance(part, (int, float)) and not isinstance(part, bool)
                        for part in value[:2])):
            x, y = float(value[0]), float(value[1])
            if math.isfinite(x) and math.isfinite(y):
                points.append((x, y))
            return
        if isinstance(value, list):
            for child in value:
                visit(child)

    if isinstance(geometry, dict):
        visit(geometry.get("coordinates"))
    if not points:
        return None
    xs, ys = zip(*points)
    return [min(xs), min(ys), max(xs), max(ys)]


def _check_path(path: Path, aoi_id: str, errors: list[str], label: str) -> None:
    for part in path.parts:
        if part in KNOWN_AOIS and part != aoi_id:
            errors.append(f"{label} mezcla AOI: la ruta contiene {part!r}, se esperaba {aoi_id!r}")


def _cell_ids_from_table(doc: Any) -> set[str]:
    if isinstance(doc, dict):
        features = doc.get("features")
        if isinstance(features, list):
            return {
                feature["properties"]["cell_id"]
                for feature in features
                if isinstance(feature, dict)
                and isinstance(feature.get("properties"), dict)
                and isinstance(feature["properties"].get("cell_id"), str)
            }
        cells = doc.get("cells", doc)
        if isinstance(cells, dict):
            return {key for key, value in cells.items() if isinstance(value, dict)}
        if isinstance(cells, list):
            return {item["cell_id"] for item in cells
                    if isinstance(item, dict) and isinstance(item.get("cell_id"), str)}
    return set()


def _contains_float(value: Any) -> bool:
    if isinstance(value, float):
        return True
    if isinstance(value, dict):
        return any(_contains_float(item) for item in value.values())
    if isinstance(value, list):
        return any(_contains_float(item) for item in value)
    return False


def _load_outputs(outputs_dir: Path, errors: list[str]) -> tuple[dict[str, dict], dict[str, Any] | None]:
    index = None
    index_path = outputs_dir / "index.json"
    if index_path.is_file():
        loaded = _read_json(index_path, errors)
        index = loaded if isinstance(loaded, dict) else None
    blocks: dict[str, dict] = {}
    wonb_dir = outputs_dir / "wonb"
    if not wonb_dir.is_dir() and outputs_dir.name == "wonb":
        wonb_dir = outputs_dir
    if wonb_dir.is_dir():
        for path in sorted(wonb_dir.rglob("*.json")):
            block = _read_json(path, errors)
            if not isinstance(block, dict):
                continue
            block_id = block.get("id")
            if not isinstance(block_id, str):
                errors.append(f"{path}: bloque sin id")
                continue
            if block_id in blocks:
                errors.append(f"{path}: id de bloque duplicado {block_id}")
            blocks[block_id] = block
    return blocks, index


def _check_block_graph(
    blocks: dict[str, dict],
    aoi_id: str,
    purpose: str,
    comparable: bool,
    errors: list[str],
) -> None:
    by_id = blocks
    has_change = has_context = False

    def parent_blocks(block: dict, expected: int | None = None) -> list[dict]:
        parents = block.get("parents")
        if not isinstance(parents, list) or (expected is not None and len(parents) != expected):
            errors.append(f"{block.get('type', 'bloque')}: cardinalidad de parents inválida")
            return []
        found = []
        for parent_id in parents:
            parent = by_id.get(parent_id) if isinstance(parent_id, str) else None
            if parent is None:
                errors.append(f"{block.get('type', 'bloque')}: padre ausente {parent_id!r}")
            else:
                found.append(parent)
        return found

    for block in blocks.values():
        kind = block.get("type")
        payload = block.get("payload")
        if not isinstance(payload, dict):
            errors.append(f"bloque {block.get('id')}: payload inválido")
            continue
        if _contains_float(payload):
            errors.append(f"{kind}: payload firmado contiene un número float")
        if kind == "change":
            has_change = True
            if payload.get("aoi_id") != aoi_id:
                errors.append(f"Change declara AOI {payload.get('aoi_id')!r}, esperado {aoi_id!r}")
        elif kind == "context":
            has_context = True
            if payload.get("aoi_id") != aoi_id:
                errors.append(f"Context declara AOI {payload.get('aoi_id')!r}, esperado {aoi_id!r}")
        elif kind == "risk":
            declared_aoi = payload.get("aoi_id")
            if declared_aoi is not None and declared_aoi != aoi_id:
                errors.append(f"Risk declara AOI {declared_aoi!r}, esperado {aoi_id!r}")
            cell_id = payload.get("cell_id", "")
            if not isinstance(cell_id, str) or not cell_id.startswith(f"{aoi_id}-r"):
                errors.append(
                    f"Risk aoi_id inferido desde cell_id {cell_id!r} no coincide con {aoi_id!r}"
                )
            parents = parent_blocks(block, 2)
            parent_types = {parent.get("type") for parent in parents}
            if parent_types != {"change", "context"}:
                errors.append(f"Risk {block.get('id')}: padres deben ser Change y Context del mismo AOI")
            for parent in parents:
                parent_payload = parent.get("payload", {})
                if parent_payload.get("aoi_id") != aoi_id:
                    errors.append(f"Risk {block.get('id')}: padre de otro AOI")
        elif kind == "action":
            declared_aoi = payload.get("aoi_id")
            if declared_aoi is not None and declared_aoi != aoi_id:
                errors.append(f"Action declara AOI {declared_aoi!r}, esperado {aoi_id!r}")
            cell_id = payload.get("cell_id", "")
            if not isinstance(cell_id, str) or not cell_id.startswith(f"{aoi_id}-r"):
                errors.append(
                    f"Action aoi_id inferido desde cell_id {cell_id!r} no coincide con {aoi_id!r}"
                )
            parents = parent_blocks(block, 1)
            if len(parents) == 1:
                risk = parents[0]
                risk_payload = risk.get("payload", {})
                if risk.get("type") != "risk":
                    errors.append(f"Action {block.get('id')}: su padre no es Risk")
                if risk_payload.get("cell_id") != cell_id:
                    errors.append(f"Action {block.get('id')}: cell_id no coincide con su Risk padre")
                risk_parents = parent_blocks(risk, 2)
                if {parent.get("type") for parent in risk_parents} != {"change", "context"}:
                    errors.append(f"Action {block.get('id')}: Risk padre no enlaza Change y Context")
                if any(parent.get("payload", {}).get("aoi_id") != aoi_id
                       for parent in risk_parents):
                    errors.append(f"Action {block.get('id')}: cadena de padres de otro AOI")
            topic = block.get("topic", "")
            audience = payload.get("audience", "")
            if aoi_id == "tailandia-01" and (
                "piura" in str(topic).casefold() or "piura" in str(audience).casefold()
            ):
                errors.append(f"Action {block.get('id')}: nombra Piura para el AOI Tailandia")

    if purpose == "scientific" and blocks and not (has_change and has_context):
        errors.append("salida científica firmada requiere Change y Context")
    has_operational = any(
        block.get("type") in {"risk", "action"} for block in blocks.values()
    )
    if not comparable and has_operational:
        errors.append("manifest.nisar.comparable=false no puede producir bloques Risk ni Action")
    if purpose != "scientific" and has_operational:
        errors.append(f"purpose={purpose} no puede contener bloques Risk ni Action")


def validate_integration(
    input_dir: Path,
    *,
    purpose: str = "scientific",
    roads_path: Path | None = None,
    inputs_dir: Path | None = None,
    outputs_dir: Path | None = None,
) -> list[str]:
    """Devuelve errores del paquete local sin escribir ni modificar archivos."""
    errors: list[str] = []
    if purpose not in PURPOSES:
        return [f"purpose debe ser uno de {sorted(PURPOSES)}"]
    input_dir = input_dir.resolve()
    manifest = _read_json(input_dir / "manifest.json", errors)
    cells = _read_json(input_dir / "cells.geojson", errors)
    if not isinstance(manifest, dict) or not isinstance(cells, dict):
        return errors

    aoi_id = manifest.get("aoi_id")
    if not isinstance(aoi_id, str) or not aoi_id:
        return errors + ["manifest.aoi_id falta o no es string"]
    _check_path(input_dir, aoi_id, errors, "paquete")
    crs = manifest.get("crs_compute")
    if not isinstance(crs, str) or not re.fullmatch(r"EPSG:(326|327)\d{2}", crs):
        errors.append("manifest.crs_compute debe declarar un CRS UTM WGS84")
    elif aoi_id in AOI_CRS and crs != AOI_CRS[aoi_id]:
        errors.append(f"{aoi_id} requiere {AOI_CRS[aoi_id]}, no {crs}")
    if cells.get("aoi_id") != aoi_id:
        errors.append(f"cells.geojson.aoi_id={cells.get('aoi_id')!r}, manifest={aoi_id!r}")
    if cells.get("crs_compute") != crs:
        errors.append(f"cells.geojson.crs_compute={cells.get('crs_compute')!r}, manifest={crs!r}")
    note = cells.get("crs_note", "")
    noted_crs = re.findall(r"EPSG:\d{5}", note) if isinstance(note, str) else []
    if noted_crs and noted_crs[-1] != crs:
        errors.append(f"cells.geojson.crs_note contradice el CRS de cálculo {crs}")

    bbox = manifest.get("bbox_wgs84")
    if (not isinstance(bbox, list) or len(bbox) != 4
            or any(not isinstance(v, (int, float)) or isinstance(v, bool)
                   or not math.isfinite(v) for v in bbox)):
        errors.append("manifest.bbox_wgs84 debe contener cuatro coordenadas")
        bbox = None

    nisar = manifest.get("nisar")
    if not isinstance(nisar, dict):
        errors.append("manifest.nisar falta o no es objeto")
        nisar = {}
    comparable = nisar.get("comparable") is True
    if purpose == "scientific":
        if not comparable:
            errors.append("purpose=scientific exige manifest.nisar.comparable=true")
        checks = nisar.get("comparable_checks")
        if not isinstance(checks, dict) or any(checks.get(name) is not True for name in COMPARABLE_CHECKS):
            errors.append("purpose=scientific exige los seis comparable_checks en true")
        scenes = [nisar.get("t1"), nisar.get("t2")]
        if any(not isinstance(scene, dict)
               or not isinstance(scene.get("granule"), str) or not scene["granule"]
               or not isinstance(scene.get("datetime"), str) or not scene["datetime"]
               for scene in scenes):
            errors.append("purpose=scientific exige granule y datetime seleccionados en t1 y t2")
        elif (scenes[0]["granule"] == scenes[1]["granule"]
              or scenes[0]["datetime"] >= scenes[1]["datetime"]):
            errors.append("purpose=scientific exige t1 y t2 distintos y en orden temporal")

    features = cells.get("features")
    if not isinstance(features, list) or not features:
        errors.append("cells.geojson.features debe ser una lista no vacía")
        features = []
    cell_ids: set[str] = set()
    for feature in features:
        if not isinstance(feature, dict) or not isinstance(feature.get("properties"), dict):
            errors.append("feature sin properties válidas")
            continue
        props = feature["properties"]
        cell_id = props.get("cell_id")
        if not isinstance(cell_id, str):
            errors.append("celda sin cell_id")
            continue
        if props.get("aoi_id") != aoi_id or not cell_id.startswith(f"{aoi_id}-r"):
            errors.append(f"celda {cell_id!r} no pertenece exclusivamente a {aoi_id!r}")
        if cell_id in cell_ids:
            errors.append(f"cell_id duplicado: {cell_id}")
        cell_ids.add(cell_id)
        for name in ("changed_fraction", "valid_fraction"):
            value = props.get(name)
            if (not isinstance(value, (int, float)) or isinstance(value, bool)
                    or not math.isfinite(value) or not 0 <= value <= 1):
                errors.append(f"{cell_id}.{name} debe estar en 0..1")
        near = props.get("road_m_near_change")
        total = props.get("road_m_total")
        if (not isinstance(near, int) or isinstance(near, bool) or near < 0
                or not isinstance(total, int) or isinstance(total, bool) or total < near):
            errors.append(f"{cell_id}: road_m_near_change/road_m_total inválidos")
        sources = props.get("source_status")
        refs = props.get("provenance_refs")
        if not isinstance(sources, dict) or set(sources) != {"nisar", "opera", "osm"}:
            errors.append(f"{cell_id}: falta source_status completo")
        else:
            origin = manifest.get("data_origin")
            if origin not in {"nisar_real", "simulated"}:
                errors.append("manifest.data_origin inválido")
            expected_nisar = "available" if origin == "nisar_real" else "simulated"
            if sources.get("nisar") != expected_nisar:
                errors.append(f"{cell_id}: estado NISAR no coincide con data_origin")
            if any(sources.get(source) not in {"available", "unavailable", "simulated"}
                   for source in ("nisar", "opera", "osm")):
                errors.append(f"{cell_id}: estado de fuente desconocido")
            opera_state = props.get("opera_status")
            if opera_state not in {"corroborated", "not_corroborated", "unavailable"}:
                errors.append(f"{cell_id}: opera_status desconocido")
            expected_opera = "unavailable" if opera_state == "unavailable" else "available"
            if sources.get("opera") != expected_opera:
                errors.append(f"{cell_id}: source_status.opera contradice opera_status")
            if sources.get("opera") == "available":
                opera_manifest = manifest.get("opera")
                if not isinstance(opera_manifest, dict) or opera_manifest.get("status") != "available":
                    errors.append(f"{cell_id}: OPERA disponible sin manifest.opera disponible")
            if sources.get("osm") == "available" and not isinstance(manifest.get("osm"), dict):
                errors.append(f"{cell_id}: OSM disponible sin metadatos en manifest.osm")
        if not isinstance(refs, dict) or set(refs) != {"nisar", "opera", "osm"}:
            errors.append(f"{cell_id}: falta provenance_refs completo")
        else:
            if not all(isinstance(refs.get(source), list) for source in ("nisar", "opera", "osm")):
                errors.append(f"{cell_id}: provenance_refs debe mapear cada fuente a una lista")
            elif isinstance(sources, dict) and any(
                sources.get(source) != "unavailable" and not refs[source]
                     for source in ("nisar", "opera", "osm")):
                errors.append(f"{cell_id}: fuente disponible sin referencia de procedencia")
        opera_ratio = props.get("opera_overlap_ratio")
        if opera_ratio is not None and (
            not isinstance(opera_ratio, (int, float))
            or isinstance(opera_ratio, bool)
            or not math.isfinite(opera_ratio)
            or not 0 <= opera_ratio <= 1
        ):
            errors.append(f"{cell_id}.opera_overlap_ratio debe ser null o estar en 0..1")
        if bbox:
            geometry_bbox = _geometry_bbox(feature.get("geometry"))
            if geometry_bbox is None or not _bbox_intersects(geometry_bbox, bbox):
                errors.append(f"{cell_id}: geometría fuera del bbox del manifest")

    companion_dir = inputs_dir.resolve() if inputs_dir else input_dir
    _check_path(companion_dir, aoi_id, errors, "insumos")
    for filename in ("grid.geojson", "opera_cells.json", "road_cells.json"):
        path = companion_dir / filename
        if not path.is_file():
            continue
        doc = _read_json(path, errors)
        if filename == "grid.geojson" and isinstance(doc, dict):
            grid_crs = (doc.get("crs") or {}).get("properties", {}).get("name", "")
            if isinstance(crs, str) and crs.split(":")[-1] not in grid_crs:
                errors.append("grid.geojson CRS contradice manifest.crs_compute")
            grid_ids = _cell_ids_from_table(doc)
            if any(not cell.startswith(f"{aoi_id}-r") for cell in grid_ids):
                errors.append("grid.geojson contiene cell_id de otro AOI")
            if cell_ids and grid_ids and not cell_ids.issubset(grid_ids):
                errors.append("cells.geojson contiene celdas que no están en grid.geojson")
        elif filename in {"opera_cells.json", "road_cells.json"} and doc is not None:
            table_ids = _cell_ids_from_table(doc)
            if any(not cell.startswith(f"{aoi_id}-r") for cell in table_ids):
                errors.append(f"{filename} contiene cell_id de otro AOI")
            if cell_ids and not table_ids.issubset(cell_ids):
                errors.append(f"{filename} contiene celdas ajenas al resultado")

    roads = roads_path.resolve() if roads_path else companion_dir / "roads.geojson"
    if roads.exists():
        _check_path(roads, aoi_id, errors, "roads")
        roads_doc = _read_json(roads, errors)
        if isinstance(roads_doc, dict):
            declared_aoi = roads_doc.get("aoi_id")
            if declared_aoi != aoi_id:
                errors.append(f"roads.geojson.aoi_id={declared_aoi!r}, esperado {aoi_id!r}")
            road_features = roads_doc.get("features")
            if not isinstance(road_features, list):
                errors.append("roads.geojson.features debe ser una lista")
            elif bbox:
                for index, feature in enumerate(road_features):
                    geometry = feature.get("geometry") if isinstance(feature, dict) else None
                    road_bbox = _geometry_bbox(geometry)
                    if road_bbox is None or not _bbox_intersects(road_bbox, bbox):
                        errors.append(
                            f"roads.geojson.features[{index}] no intersecta el bbox del AOI"
                        )
                    elif (
                        road_bbox[0] < bbox[0]
                        or road_bbox[1] < bbox[1]
                        or road_bbox[2] > bbox[2]
                        or road_bbox[3] > bbox[3]
                    ):
                        errors.append(
                            f"roads.geojson.features[{index}] excede el AOI; "
                            "recorte cada geometría antes de integrarla"
                        )

    metadata_path = companion_dir / "osm-metadata.json"
    if metadata_path.is_file():
        metadata = _read_json(metadata_path, errors)
        if isinstance(metadata, dict):
            if metadata.get("aoi_id") not in (None, aoi_id):
                errors.append(f"osm-metadata.json.aoi_id no coincide con {aoi_id}")
            metadata_bbox = metadata.get("bbox_wgs84")
            if bbox and isinstance(metadata_bbox, list) and len(metadata_bbox) == 4:
                if not _bbox_intersects(metadata_bbox, bbox):
                    errors.append("osm-metadata.json bbox pertenece a otro AOI")

    output_root = (outputs_dir or input_dir).resolve()
    _check_path(output_root, aoi_id, errors, "outputs")
    blocks, index = _load_outputs(output_root, errors)
    if index is not None:
        if index.get("aoi_id") != aoi_id:
            errors.append(f"index.json.aoi_id={index.get('aoi_id')!r}, manifest={aoi_id!r}")
        index_purpose = index.get("purpose")
        if index_purpose is not None and index_purpose != purpose:
            errors.append(f"index.json.purpose={index_purpose!r}, solicitado={purpose!r}")
        if purpose != "scientific" and index_purpose != purpose:
            errors.append(f"bundle {purpose} requiere index.json.purpose explícito")
        for entry in index.get("files", []):
            relative = entry.get("path", "") if isinstance(entry, dict) else ""
            if isinstance(relative, str):
                _check_path(Path(relative), aoi_id, errors, "index")
    for block in blocks.values():
        block_type = block.get("type")
        if block_type in {"change", "context", "risk", "action"}:
            topic = str(block.get("topic", ""))
            if f"/{aoi_id}/" not in topic:
                errors.append(f"{block_type} topic no está aislado bajo {aoi_id}")
            _check_path(Path(topic), aoi_id, errors, "topic")
            if aoi_id == "tailandia-01" and "piura" in topic.casefold():
                errors.append(f"{block_type} topic mezcla Piura con el AOI Tailandia")
            for aoi in KNOWN_AOIS - {aoi_id}:
                if aoi in str(block.get("topic", "")) or aoi in str(block.get("payload", {})):
                    errors.append(f"{block_type} contiene referencia al AOI ajeno {aoi}")

    _check_block_graph(blocks, aoi_id, purpose, comparable, errors)
    return errors


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--in", dest="input_dir", type=Path, required=True,
                        help="directorio con manifest.json y cells.geojson")
    parser.add_argument("--purpose", choices=sorted(PURPOSES), default="scientific",
                        help="scientific exige comparable=true; baseline/preparation prohíben Risk/Action")
    parser.add_argument("--roads", type=Path, help="roads.geojson local, opcional")
    parser.add_argument("--inputs", type=Path, help="directorio local con grid y tablas por celda")
    parser.add_argument("--outputs", type=Path, help="directorio con index.json y/o wonb/")
    args = parser.parse_args(argv)
    errors = validate_integration(
        args.input_dir,
        purpose=args.purpose,
        roads_path=args.roads,
        inputs_dir=args.inputs,
        outputs_dir=args.outputs,
    )
    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1
    print(f"OK: integración aislada para {args.input_dir} ({args.purpose})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
