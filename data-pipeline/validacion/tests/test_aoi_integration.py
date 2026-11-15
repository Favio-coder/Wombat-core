import json
from pathlib import Path

from validate_aoi_integration import validate_integration


def _package(root, *, aoi_id="tailandia-01", comparable=True):
    root.mkdir(parents=True, exist_ok=True)
    checks = {
        "same_frame": True,
        "same_orbit_direction": True,
        "same_mode": True,
        "same_frequency": True,
        "same_polarization": True,
        "same_maturity": True,
    }
    manifest = {
        "aoi_id": aoi_id,
        "data_origin": "nisar_real",
        "crs_compute": "EPSG:32647" if aoi_id == "tailandia-01" else "EPSG:32717",
        "bbox_wgs84": [100.3, 14.3, 100.5, 14.5] if aoi_id == "tailandia-01"
        else [-80.8, -5.3, -80.5, -5.1],
        "nisar": {
            "comparable": comparable,
            "comparable_checks": checks,
            "t1": {"granule": "granule-1", "datetime": "2026-09-18T10:00:00Z"},
            "t2": {"granule": "granule-2", "datetime": "2026-09-30T10:00:00Z"},
        },
    }
    bbox = manifest["bbox_wgs84"]
    cell_id = f"{aoi_id}-r00c00"
    cells = {
        "type": "FeatureCollection",
        "aoi_id": aoi_id,
        "crs_compute": manifest["crs_compute"],
        "crs_note": f"geometry EPSG:4326; calculos hechos en {manifest['crs_compute']}",
        "features": [{
            "type": "Feature",
            "geometry": {
                "type": "Polygon",
                "coordinates": [[[bbox[0], bbox[1]], [bbox[2], bbox[1]],
                                 [bbox[2], bbox[3]], [bbox[0], bbox[3]],
                                 [bbox[0], bbox[1]]]],
            },
            "properties": {
                "aoi_id": aoi_id,
                "cell_id": cell_id,
                "changed_fraction": 0.1,
                "valid_fraction": 0.9,
                "decrease_fraction": 0.05,
                "increase_fraction": 0.05,
                "p90_abs_delta_db": 1.25,
                "opera_status": "unavailable",
                "opera_overlap_ratio": None,
                "road_m_near_change": 0,
                "road_m_total": 0,
                "source_status": {
                    "nisar": "available",
                    "opera": "unavailable",
                    "osm": "unavailable",
                },
                "provenance_refs": {
                    "nisar": ["manifest.json#/nisar"],
                    "opera": ["manifest.json#/opera"],
                    "osm": [],
                },
            },
        }],
    }
    (root / "manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
    (root / "cells.geojson").write_text(json.dumps(cells), encoding="utf-8")
    return cell_id


def test_consistent_aoi_package_is_accepted(tmp_path):
    package = tmp_path / "tailandia-01" / "evento"
    _package(package)

    assert validate_integration(package) == []


def test_piura_cells_cannot_be_mixed_into_tailandia_package(tmp_path):
    package = tmp_path / "tailandia-01" / "evento"
    _package(package, aoi_id="piura-01")
    cells = json.loads((package / "cells.geojson").read_text(encoding="utf-8"))
    cells["aoi_id"] = "tailandia-01"
    cells["features"][0]["properties"]["aoi_id"] = "tailandia-01"
    (package / "cells.geojson").write_text(json.dumps(cells), encoding="utf-8")

    errors = validate_integration(package)

    assert any("cells.geojson.aoi_id" in error for error in errors)
    assert any("no pertenece exclusivamente" in error for error in errors)
    assert any("ruta contiene 'tailandia-01'" in error for error in errors)


def test_non_comparable_package_cannot_contain_risk_or_action(tmp_path):
    package = tmp_path / "tailandia-01" / "preparacion"
    cell_id = _package(package, comparable=False)
    wonb = package / "wonb"
    (wonb / "risk").mkdir(parents=True)
    (wonb / "action").mkdir()
    risk_id = "sha256:" + "1" * 64
    (wonb / "risk" / "risk.json").write_text(json.dumps({
        "id": risk_id,
        "type": "risk",
        "topic": f"wombat/aoi/tailandia-01/risk",
        "parents": [],
        "payload": {"cell_id": cell_id},
    }), encoding="utf-8")
    (wonb / "action" / "action.json").write_text(json.dumps({
        "id": "sha256:" + "2" * 64,
        "type": "action",
        "topic": "wombat/aoi/tailandia-01/action",
        "parents": [risk_id],
        "payload": {"cell_id": cell_id, "audience": "local operator"},
    }), encoding="utf-8")

    errors = validate_integration(package, purpose="preparation")

    assert any("no puede contener bloques Risk ni Action" in error for error in errors)
    scientific_errors = validate_integration(package, purpose="scientific")
    assert any("comparable=false no puede producir bloques Risk ni Action" in error
               for error in scientific_errors)


def test_risk_and_action_cell_identity_must_match_parent_aoi(tmp_path):
    package = tmp_path / "tailandia-01" / "evento"
    _package(package)
    wonb = package / "wonb"
    (wonb / "risk").mkdir(parents=True)
    (wonb / "action").mkdir()
    change_id, context_id = "change-id", "context-id"
    risk_id = "risk-id"
    cell_id = "piura-01-r00c00"
    _write_block(wonb / "change.json", change_id, "change", "tailandia-01",
                 payload={"aoi_id": "tailandia-01"})
    _write_block(wonb / "context.json", context_id, "context", "tailandia-01",
                 payload={"aoi_id": "tailandia-01"})
    _write_block(wonb / "risk" / "risk.json", risk_id, "risk", "tailandia-01",
                 parents=[change_id, context_id], payload={"cell_id": cell_id})
    _write_block(wonb / "action" / "action.json", "action-id", "action", "tailandia-01",
                 parents=[risk_id], payload={"cell_id": cell_id, "audience": "local operator"})

    errors = validate_integration(package)

    assert any("Risk aoi_id inferido desde cell_id" in error for error in errors)
    assert any("Action aoi_id inferido desde cell_id" in error for error in errors)


def test_baseline_without_operational_blocks_is_allowed_when_not_comparable(tmp_path):
    package = tmp_path / "piura-01" / "baseline"
    _package(package, aoi_id="piura-01", comparable=False)

    assert validate_integration(package, purpose="baseline") == []
    assert any("comparable=true" in error
               for error in validate_integration(package, purpose="scientific"))


def test_known_aoi_rejects_wrong_compute_crs(tmp_path):
    package = tmp_path / "tailandia-01" / "evento"
    _package(package)
    manifest = json.loads((package / "manifest.json").read_text(encoding="utf-8"))
    manifest["crs_compute"] = "EPSG:32717"
    (package / "manifest.json").write_text(json.dumps(manifest), encoding="utf-8")

    errors = validate_integration(package)

    assert any("tailandia-01 requiere EPSG:32647" in error for error in errors)


def test_integrated_cell_validates_against_contract_schema():
    from jsonschema import Draft202012Validator

    schema_path = Path(__file__).parents[3] / "contracts" / "v1" / "cells.schema.json"
    schema = json.loads(schema_path.read_text(encoding="utf-8"))
    package = Path(__file__).parent
    cell_package = package / "fixtures" / "aoi_integration" / "integrated_cell.json"
    instance = json.loads(cell_package.read_text(encoding="utf-8"))

    Draft202012Validator.check_schema(schema)
    Draft202012Validator(schema).validate(instance)


def test_index_requires_purpose_for_12_but_accepts_legacy_shape():
    from jsonschema import Draft202012Validator

    schema_path = Path(__file__).parents[3] / "contracts" / "v1" / "bundle-index.schema.json"
    schema = json.loads(schema_path.read_text(encoding="utf-8"))
    validator = Draft202012Validator(schema)
    index = {
        "contract_version": "1.0.0",
        "bundle_version": "v0",
        "aoi_id": "piura-demo",
        "data_origin": "simulated",
        "generated_at": "2026-11-14T17:00:00Z",
        "core_commit": "fixture",
        "files": [{"path": "manifest.json", "sha256": "0" * 64}],
    }

    assert validator.is_valid(index)
    index["contract_version"] = "1.2.0"
    assert not validator.is_valid(index)
    index["purpose"] = "baseline"
    assert validator.is_valid(index)


def test_roads_from_another_aoi_are_rejected_individually(tmp_path):
    package = tmp_path / "tailandia-01" / "evento"
    _package(package)
    roads = package / "roads.geojson"
    roads.write_text(json.dumps({
        "type": "FeatureCollection",
        "features": [
            {"type": "Feature", "properties": {}, "geometry": {
                "type": "LineString", "coordinates": [[100.35, 14.35], [100.4, 14.4]]
            }},
            {"type": "Feature", "properties": {}, "geometry": {
                "type": "LineString", "coordinates": [[-80.7, -5.2], [-80.6, -5.2]]
            }},
        ],
    }), encoding="utf-8")

    errors = validate_integration(package, roads_path=roads)

    assert any("features[1] no intersecta el bbox" in error for error in errors)


def test_unclipped_road_crossing_aoi_bbox_is_rejected(tmp_path):
    package = tmp_path / "tailandia-01" / "evento"
    _package(package)
    roads = package / "roads.geojson"
    roads.write_text(json.dumps({
        "type": "FeatureCollection",
        "aoi_id": "tailandia-01",
        "features": [{
            "type": "Feature",
            "properties": {},
            "geometry": {
                "type": "LineString",
                "coordinates": [[100.4, 14.4], [-80.6, -5.2]],
            },
        }],
    }), encoding="utf-8")

    errors = validate_integration(package, roads_path=roads)

    assert any("excede el AOI" in error for error in errors)


def _write_block(path, block_id, block_type, aoi_id, *, parents=None, payload=None):
    path.write_text(json.dumps({
        "id": block_id,
        "type": block_type,
        "topic": f"wombat/aoi/{aoi_id}/{block_type}",
        "parents": parents or [],
        "payload": payload or {},
    }), encoding="utf-8")
