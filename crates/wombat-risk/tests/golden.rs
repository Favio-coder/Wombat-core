//! El ranking dorado: `wombat-risk` sobre el fixture tiene que dar exactamente
//! lo que ya está publicado y firmado.
//!
//! Dos referencias independientes, y las dos tienen que cuadrar:
//!
//! - `contracts/v1/fixtures/expected/ranking.json`, el ranking de referencia;
//! - los 16 bloques Risk firmados de `piura-demo/v0`, celda por celda.
//!
//! Si este test falla, el ranking cambió. Y un ranking que cambia después de
//! publicado significa que alguien verificó en campo una celda y el bundle dice
//! otra cosa, así que el golden **no se toca**: se arregla el código.
//!
//! `contracts/v1/` es de solo lectura desde aquí (R20).

use std::path::{Path, PathBuf};

use wombat_core::v02::{
    generate_ephemeral_keypair, issuer_string, Block, BlockBody, BlockType, ChangePayload,
    ContextPayload, DataOrigin, Frequency, OverlayHashes, Payload, Provenance, RiskStatus, Source,
};
use wombat_risk::{
    build_action_bodies, build_action_payloads, build_risk_bodies, build_risk_payloads, evaluate,
    load, BlockContext, CellRisk, RiskConfig,
};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn bundle_dir() -> PathBuf {
    repo_root().join("contracts/v1/fixtures/bundle/piura-demo/v0")
}

/// La config y las celdas del propio bundle: es lo que un verificador tiene.
fn fixture_config() -> RiskConfig {
    load::load_config(bundle_dir().join("risk-config.json")).unwrap()
}

fn fixture_cells() -> Vec<CellRisk> {
    let inputs = load::load_inputs("piura-demo", bundle_dir().join("cells.geojson")).unwrap();
    evaluate(&fixture_config(), &inputs).unwrap()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", path.display()))
}

/// Los 16 bloques Risk del bundle, por `cell_id`.
fn fixture_risk_payloads() -> Vec<(String, serde_json::Value)> {
    let dir = bundle_dir().join("wonb/risk");
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    entries
        .iter()
        .map(|path| {
            let block: serde_json::Value = serde_json::from_str(&read(path)).unwrap();
            let payload = block["payload"].clone();
            (payload["cell_id"].as_str().unwrap().to_string(), payload)
        })
        .collect()
}

#[test]
fn ranking_matches_the_golden_file() {
    let golden: serde_json::Value = serde_json::from_str(&read(
        &repo_root().join("contracts/v1/fixtures/expected/ranking.json"),
    ))
    .unwrap();
    let cells = fixture_cells();

    let ranked: Vec<&CellRisk> = cells.iter().filter(|c| c.is_ranked()).collect();
    let golden_ranking = golden["ranking"].as_array().unwrap();
    assert_eq!(ranked.len(), golden_ranking.len(), "celdas en el ranking");

    for (cell, want) in ranked.iter().zip(golden_ranking) {
        let where_ = format!("celda {}", cell.cell_id);
        assert_eq!(cell.cell_id, want["cell_id"].as_str().unwrap(), "{where_}");
        assert_eq!(
            cell.rank,
            want["rank"].as_u64().map(|v| v as u32),
            "{where_}"
        );
        assert_eq!(
            cell.score_bp,
            want["score_bp"].as_u64().map(|v| v as u32),
            "{where_}"
        );
        let components = cell.components_bp.as_ref().unwrap();
        let want_components = &want["components_bp"];
        assert_eq!(
            u64::from(components.change),
            want_components["change"].as_u64().unwrap(),
            "{where_}"
        );
        assert_eq!(
            u64::from(components.opera),
            want_components["opera"].as_u64().unwrap(),
            "{where_}"
        );
        assert_eq!(
            u64::from(components.road),
            want_components["road"].as_u64().unwrap(),
            "{where_}"
        );
        assert_eq!(
            serde_json::to_value(cell.status).unwrap(),
            want["status"],
            "{where_}"
        );
    }

    let excluded: Vec<&str> = cells
        .iter()
        .filter(|c| !c.is_ranked())
        .map(|c| c.cell_id.as_str())
        .collect();
    let want_excluded: Vec<&str> = golden["insufficient_quality"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(excluded, want_excluded);
}

#[test]
fn the_top_three_and_the_status_mix_are_what_the_plan_says() {
    let cells = fixture_cells();
    let top: Vec<(&str, Option<u32>)> = cells
        .iter()
        .take(3)
        .map(|c| (c.cell_id.as_str(), c.score_bp))
        .collect();
    assert_eq!(
        top,
        [
            ("piura-demo-r02c01", Some(7636)),
            ("piura-demo-r01c01", Some(6593)),
            ("piura-demo-r02c02", Some(6365)),
        ]
    );

    let count = |status| cells.iter().filter(|c| c.status == status).count();
    assert_eq!(count(RiskStatus::ReviewHigh), 3);
    assert_eq!(count(RiskStatus::ReviewMedium), 3);
    assert_eq!(count(RiskStatus::ReviewLow), 9);
    assert_eq!(count(RiskStatus::InsufficientQuality), 1);
    assert_eq!(cells.len(), 16);
}

#[test]
fn every_one_of_the_16_scores_matches_its_signed_block() {
    let cells = fixture_cells();
    let fixture = fixture_risk_payloads();
    assert_eq!(fixture.len(), 16);

    for (cell_id, want) in &fixture {
        let cell = cells
            .iter()
            .find(|c| &c.cell_id == cell_id)
            .unwrap_or_else(|| panic!("{cell_id} no fue evaluada"));
        assert_eq!(
            cell.score_bp,
            want["score_bp"].as_u64().map(|v| v as u32),
            "score_bp de {cell_id}"
        );
        assert_eq!(
            cell.rank,
            want["rank"].as_u64().map(|v| v as u32),
            "rank de {cell_id}"
        );
        assert_eq!(
            serde_json::to_value(cell.status).unwrap(),
            want["status"],
            "status de {cell_id}"
        );
    }
}

#[test]
fn every_risk_payload_is_byte_for_byte_the_one_in_the_bundle() {
    // La comparación es sobre el JCS del payload, que es lo que se firma. No se
    // comparan ids ni firmas: cambian con el emisor y el momento de emisión.
    let cfg = fixture_config();
    let cells = fixture_cells();
    let payloads = build_risk_payloads(&cfg, &cells).unwrap();

    for (cell, payload) in cells.iter().zip(&payloads) {
        let want = fixture_risk_payloads()
            .into_iter()
            .find(|(id, _)| id == &cell.cell_id)
            .map(|(_, payload)| payload)
            .unwrap_or_else(|| panic!("{} no está en el bundle", cell.cell_id));

        let ours = serde_jcs::to_vec(payload).unwrap();
        let theirs = serde_jcs::to_vec(&want).unwrap();
        assert_eq!(
            String::from_utf8(ours).unwrap(),
            String::from_utf8(theirs).unwrap(),
            "payload de {}",
            cell.cell_id
        );
    }
}

#[test]
fn every_action_payload_is_byte_for_byte_the_one_in_the_bundle() {
    let cfg = fixture_config();
    let cells = fixture_cells();
    let ours = build_action_payloads(&cfg, &cells).unwrap();

    let dir = bundle_dir().join("wonb/action");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 3, "el fixture tiene action/1-3");

    for (path, action) in paths.iter().zip(&ours) {
        let block: serde_json::Value = serde_json::from_str(&read(path)).unwrap();
        assert_eq!(
            String::from_utf8(serde_jcs::to_vec(action).unwrap()).unwrap(),
            String::from_utf8(serde_jcs::to_vec(&block["payload"]).unwrap()).unwrap(),
            "{}",
            path.display()
        );
    }
}

#[test]
fn sealing_the_fixture_ranking_produces_a_chain_that_verifies() {
    let cfg = fixture_config();
    let cells = fixture_cells();
    let (key, vk) = generate_ephemeral_keypair();

    let mut ctx = BlockContext {
        aoi_id: "piura-demo".into(),
        topic_prefix: "wombat/pe/piura".into(),
        issued_at: "2026-11-14T17:00:00Z".into(),
        issuer: issuer_string(&vk),
        change_id: String::new(),
        context_id: String::new(),
        provenance: Provenance {
            sources: vec![Source {
                product: "NISAR_L2_GCOV".into(),
                extra: Default::default(),
            }],
            method: cfg.version.clone(),
            code_commit: "test".into(),
        },
    };

    let change = Block::seal(root_body(&ctx, BlockType::Change), &key).unwrap();
    let context = Block::seal(root_body(&ctx, BlockType::Context), &key).unwrap();
    ctx.change_id = change.id.clone();
    ctx.context_id = context.id.clone();

    let risks: Vec<Block> = build_risk_bodies(&cfg, &cells, &ctx)
        .unwrap()
        .into_iter()
        .map(|body| Block::seal(body, &key).unwrap())
        .collect();
    let actions: Vec<Block> = build_action_bodies(&cfg, &risks, &ctx)
        .unwrap()
        .into_iter()
        .map(|body| Block::seal(body, &key).unwrap())
        .collect();

    // El mismo recuento del bundle: 1 change + 1 context + 16 risk + 3 action.
    assert_eq!(risks.len(), 16);
    assert_eq!(actions.len(), 3);

    let mut all = vec![change, context];
    all.extend(risks);
    all.extend(actions);
    assert_eq!(all.len(), 21);
    wombat_core::v02::verify_chain(&all).unwrap();
}

#[test]
fn two_runs_over_the_fixture_are_identical_byte_for_byte() {
    let cfg = fixture_config();
    let first = build_risk_payloads(&cfg, &fixture_cells()).unwrap();
    let second = build_risk_payloads(&cfg, &fixture_cells()).unwrap();
    assert_eq!(
        serde_jcs::to_vec(&first).unwrap(),
        serde_jcs::to_vec(&second).unwrap()
    );
}

/// Un Change o un Context mínimo, solo para que los Risk tengan de qué colgar.
fn root_body(ctx: &BlockContext, block_type: BlockType) -> BlockBody {
    let payload = match block_type {
        BlockType::Change => Payload::Change(ChangePayload {
            aoi_id: ctx.aoi_id.clone(),
            data_origin: DataOrigin::Simulated,
            t1: "2026-10-14T23:00:00Z".into(),
            t2: "2026-10-26T23:00:00Z".into(),
            polarization: "HHHH".into(),
            frequency: Frequency::A,
            pixel_spacing_m: 20,
            delta_db_threshold_cdb: 300,
            aoi_valid_bp: 9025,
            aoi_decrease_bp: 906,
            aoi_increase_bp: 275,
            overlay_sha256: OverlayHashes {
                t1_png: "11".repeat(32),
                t2_png: "22".repeat(32),
                delta_png: "33".repeat(32),
            },
            cells_geojson_sha256: "44".repeat(32),
        }),
        _ => Payload::Context(ContextPayload {
            aoi_id: ctx.aoi_id.clone(),
            roads_total_m: 16242,
            road_buffer_m: 250,
            osm_retrieved_at: "2026-11-14T15:00:00Z".into(),
            roads_geojson_sha256: "55".repeat(32),
        }),
    };
    BlockBody {
        wonb_version: wombat_core::v02::WONB_VERSION.to_string(),
        block_type,
        topic: format!("wombat/pe/piura/{}/{block_type}", ctx.aoi_id),
        issued_at: ctx.issued_at.clone(),
        valid_until: None,
        issuer: ctx.issuer.clone(),
        parents: Vec::new(),
        payload,
        provenance: ctx.provenance.clone(),
    }
}
