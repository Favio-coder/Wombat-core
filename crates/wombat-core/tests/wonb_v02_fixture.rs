//! Verificación de los 21 bloques del bundle fixture `piura-demo/v0`.
//!
//! Esta es la prueba cruzada Python↔Rust del contrato v1: el fixture lo firmó
//! `contracts/v1/tools/gen_fixture_bundle.py` y lo verifica
//! `verify_bundle.py`. Si este test pasa, las dos implementaciones
//! canonicalizan igual (JCS, RFC 8785) y firman lo mismo (los 32 bytes del
//! hash). Si falla, el bug está en la canonicalización, no en el fixture.
//!
//! El bundle es **de solo lectura** para este crate: R20 dice que `contracts/`
//! solo cambia por un PR que toque esquema, fixture y `VERSION` a la vez.

use std::path::{Path, PathBuf};

use wombat_core::v02::{Block, BlockType, TrustedIssuers, WonbError};

/// Raíz del bundle fixture, relativa al crate.
fn bundle_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts/v1/fixtures/bundle/piura-demo/v0")
}

/// Instante contra el que se mide el TTL: el `generated_at` del índice.
///
/// Un bundle estático es una foto histórica. Evaluado con la hora actual, todo
/// `risk` y `action` saldría expirado (ver `contracts/v1/README.md`, "TTL en
/// bundles estáticos"), y este test empezaría a fallar solo por el paso del
/// tiempo.
fn generated_at() -> String {
    let index: serde_json::Value =
        serde_json::from_str(&read(&bundle_dir().join("index.json"))).unwrap();
    index["generated_at"].as_str().unwrap().to_string()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", path.display()))
}

fn trusted_issuers() -> TrustedIssuers {
    TrustedIssuers::from_json(&read(&bundle_dir().join("trusted-issuers.json"))).unwrap()
}

/// Todos los `.json` bajo `wonb/`, en orden estable.
fn block_paths() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let entries = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("no se pudo listar {}: {e}", dir.display()));
        for entry in entries {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "json") {
                out.push(path);
            }
        }
    }
    let mut paths = Vec::new();
    walk(&bundle_dir().join("wonb"), &mut paths);
    paths.sort();
    paths
}

fn load_blocks() -> Vec<Block> {
    block_paths()
        .iter()
        .map(|p| Block::from_json(&read(p)).unwrap_or_else(|e| panic!("{}: {e}", p.display())))
        .collect()
}

#[test]
fn all_21_fixture_blocks_verify() {
    let trusted = trusted_issuers();
    let as_of = generated_at();
    let paths = block_paths();
    assert_eq!(paths.len(), 21, "el bundle debe tener 21 bloques");

    for path in &paths {
        let block =
            Block::from_json(&read(path)).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        block
            .verify(Some(&as_of), &trusted)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    }
}

#[test]
fn fixture_has_the_expected_mix_of_types() {
    let blocks = load_blocks();
    let count = |t: BlockType| blocks.iter().filter(|b| b.body.block_type == t).count();
    assert_eq!(count(BlockType::Change), 1);
    assert_eq!(count(BlockType::Context), 1);
    assert_eq!(count(BlockType::Risk), 16, "16 celdas");
    assert_eq!(count(BlockType::Action), 3, "top 3 del ranking");
}

#[test]
fn fixture_chain_is_complete() {
    verify_chain_of(&load_blocks());
}

fn verify_chain_of(blocks: &[Block]) {
    wombat_core::v02::verify_chain(blocks).unwrap();
}

#[test]
fn tampering_one_byte_of_a_payload_breaks_the_id() {
    let trusted = trusted_issuers();
    let as_of = generated_at();
    let path = bundle_dir().join("wonb/risk/piura-demo-r02c01.json");
    let mut raw: serde_json::Value = serde_json::from_str(&read(&path)).unwrap();
    let before = raw["payload"]["metrics"]["valid_bp"].as_u64().unwrap();
    raw["payload"]["metrics"]["valid_bp"] = serde_json::json!(before + 1);

    let block = Block::from_value(raw).unwrap();
    let err = block.verify(Some(&as_of), &trusted).unwrap_err();
    assert!(matches!(err, WonbError::IdMismatch { .. }), "{err}");
}

#[test]
fn replacing_the_signature_breaks_the_signature_check() {
    let trusted = trusted_issuers();
    let as_of = generated_at();
    let path = bundle_dir().join("wonb/change.json");
    let mut raw: serde_json::Value = serde_json::from_str(&read(&path)).unwrap();
    // Otra firma del largo correcto. El `id` sigue válido: la firma no entra
    // en el cuerpo firmado.
    raw["signature"] = serde_json::json!("ab".repeat(64));

    let block = Block::from_value(raw).unwrap();
    let err = block.verify(Some(&as_of), &trusted).unwrap_err();
    assert!(matches!(err, WonbError::BadSignature { .. }), "{err}");
}

#[test]
fn fixture_issuer_is_rejected_against_an_empty_trust_list() {
    let as_of = generated_at();
    let block = &load_blocks()[0];
    let err = block
        .verify(Some(&as_of), &TrustedIssuers::empty())
        .unwrap_err();
    assert!(matches!(err, WonbError::UntrustedIssuer { .. }), "{err}");
}

#[test]
fn fixture_risk_blocks_expire_25h_after_being_issued() {
    let trusted = trusted_issuers();
    let path = bundle_dir().join("wonb/risk/piura-demo-r02c01.json");
    let block = Block::from_json(&read(&path)).unwrap();

    // Emitido 2026-11-14T17:00:00Z, válido 24 h.
    block
        .verify(Some("2026-11-15T16:00:00Z"), &trusted)
        .unwrap();
    let err = block
        .verify(Some("2026-11-15T18:00:00Z"), &trusted)
        .unwrap_err();
    assert!(matches!(err, WonbError::Expired { .. }), "{err}");
}

#[test]
fn fixture_change_block_never_expires() {
    let trusted = trusted_issuers();
    let block = Block::from_json(&read(&bundle_dir().join("wonb/change.json"))).unwrap();
    block
        .verify(Some("2099-01-01T00:00:00Z"), &trusted)
        .unwrap();
}

#[test]
fn a_risk_block_alone_has_no_parents_to_point_at() {
    let risk = Block::from_json(&read(
        &bundle_dir().join("wonb/risk/piura-demo-r02c01.json"),
    ))
    .unwrap();
    let err = wombat_core::v02::verify_chain(&[risk]).unwrap_err();
    assert!(matches!(err, WonbError::MissingParent { .. }), "{err}");
}

#[test]
fn a_float_in_the_payload_is_rejected_before_anything_else() {
    let path = bundle_dir().join("wonb/risk/piura-demo-r02c01.json");
    let mut raw: serde_json::Value = serde_json::from_str(&read(&path)).unwrap();
    raw["payload"]["score_bp"] = serde_json::json!(0.5);

    let err = Block::from_value(raw).unwrap_err();
    match err {
        WonbError::FloatInPayload { path } => assert_eq!(path, "payload.score_bp"),
        other => panic!("se esperaba FloatInPayload, llegó {other}"),
    }
}

#[test]
fn every_fixture_block_roundtrips_to_the_same_bytes() {
    // Si al reserializar cambiara un solo byte, el `id` no cuadraría: esto
    // prueba que los tipos de Rust no pierden ni reordenan nada del contrato.
    for path in block_paths() {
        let block = Block::from_json(&read(&path)).unwrap();
        let reserialized = serde_json::to_string(&block).unwrap();
        let reparsed =
            Block::from_json(&reserialized).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(
            block.body.compute_id().unwrap(),
            reparsed.body.compute_id().unwrap(),
            "{}",
            path.display()
        );
        assert_eq!(block, reparsed, "{}", path.display());
    }
}
