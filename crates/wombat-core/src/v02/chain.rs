//! Cadena de bloques: Change/Context → Risk → Action.
//!
//! Verificar bloques uno a uno no basta. Un `risk` firmado y válido que apunte a
//! un `change` que nadie publicó no es evidencia de nada: la cadena es lo que
//! permite recorrer un juicio hasta la medición que lo originó.
//!
//! Lo que se exige:
//!
//! - cada `parent` declarado existe en el conjunto;
//! - el tipo del padre es admisible para el tipo del hijo
//!   ([`BlockType::allowed_parent_types`]);
//! - un `risk` tiene exactamente un `change` y un `context`;
//! - un `action` tiene exactamente un `risk`;
//! - `change` y `context` no tienen padres (son la raíz).

use std::collections::HashMap;

use super::envelope::{Block, BlockType};
use super::error::WonbError;

/// Verifica la cadena de un conjunto de bloques.
///
/// No vuelve a comprobar firmas: eso es [`Block::verify`]. Aquí solo se mira la
/// topología, para que los dos fallos se puedan distinguir.
pub fn verify_chain(blocks: &[Block]) -> Result<(), WonbError> {
    let by_id: HashMap<&str, &Block> = blocks.iter().map(|b| (b.id.as_str(), b)).collect();

    for block in blocks {
        let allowed = block.body.block_type.allowed_parent_types();

        if allowed.is_empty() && !block.body.parents.is_empty() {
            return Err(WonbError::Schema(format!(
                "un bloque {} es raíz y no puede tener padres, pero declara {}",
                block.body.block_type,
                block.body.parents.len()
            )));
        }

        let mut parent_types = Vec::with_capacity(block.body.parents.len());
        for parent_id in &block.body.parents {
            let parent = by_id
                .get(parent_id.as_str())
                .ok_or_else(|| WonbError::MissingParent {
                    id: block.id.clone(),
                    parent: parent_id.clone(),
                })?;
            if !allowed.contains(&parent.body.block_type) {
                return Err(WonbError::Schema(format!(
                    "un bloque {} no puede tener un padre {}",
                    block.body.block_type, parent.body.block_type
                )));
            }
            parent_types.push(parent.body.block_type);
        }

        check_parent_set(block, &parent_types)?;
    }

    Ok(())
}

/// Comprueba la cantidad exacta de padres de cada tipo que pide el contrato.
fn check_parent_set(block: &Block, parent_types: &[BlockType]) -> Result<(), WonbError> {
    let count = |t: BlockType| parent_types.iter().filter(|p| **p == t).count();

    let expected: &[(BlockType, usize)] = match block.body.block_type {
        BlockType::Change | BlockType::Context => &[],
        BlockType::Risk => &[(BlockType::Change, 1), (BlockType::Context, 1)],
        BlockType::Action => &[(BlockType::Risk, 1)],
    };

    for (parent_type, want) in expected {
        let got = count(*parent_type);
        if got != *want {
            return Err(WonbError::Schema(format!(
                "un bloque {} necesita {want} padre(s) {parent_type}, tiene {got}",
                block.body.block_type
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v02::sign::{generate_ephemeral_keypair, issuer_string};
    use serde_json::{json, Value};

    /// Sella un bloque del tipo pedido, con los padres dados.
    fn seal(block_type: &str, parents: Vec<String>, payload: Value) -> Block {
        let (key, vk) = generate_ephemeral_keypair();
        let valid_until = match block_type {
            "risk" => json!("2026-11-15T17:00:00Z"),
            "action" => json!("2026-11-14T18:00:00Z"),
            _ => Value::Null,
        };
        let raw = json!({
            "wonb_version": "0.2",
            "id": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "type": block_type,
            "topic": format!("wombat/pe/piura/piura-demo/{block_type}"),
            "issued_at": "2026-11-14T17:00:00Z",
            "valid_until": valid_until,
            "issuer": issuer_string(&vk),
            "parents": parents,
            "payload": payload,
            "provenance": {
                "sources": [{"product": "NISAR_L2_GCOV"}],
                "method": "risk-config/0.1",
                "code_commit": "test"
            },
            "signature": "00"
        });
        Block::seal(Block::from_value(raw).unwrap().body, &key).unwrap()
    }

    fn change() -> Block {
        seal(
            "change",
            vec![],
            json!({
                "aoi_id": "piura-demo", "data_origin": "simulated",
                "t1": "2026-10-14T23:00:00Z", "t2": "2026-10-26T23:00:00Z",
                "polarization": "HHHH", "frequency": "A", "pixel_spacing_m": 20,
                "delta_db_threshold_cdb": 300, "aoi_valid_bp": 9025,
                "aoi_decrease_bp": 906, "aoi_increase_bp": 275,
                "overlay_sha256": {"t1.png": "aa".repeat(32), "t2.png": "bb".repeat(32), "delta.png": "cc".repeat(32)},
                "cells_geojson_sha256": "dd".repeat(32)
            }),
        )
    }

    fn context() -> Block {
        seal(
            "context",
            vec![],
            json!({
                "aoi_id": "piura-demo", "roads_total_m": 16242, "road_buffer_m": 250,
                "osm_retrieved_at": "2026-11-14T15:00:00Z",
                "roads_geojson_sha256": "ee".repeat(32)
            }),
        )
    }

    fn risk(parents: Vec<String>) -> Block {
        seal(
            "risk",
            parents,
            json!({
                "cell_id": "piura-demo-r02c01", "status": "review_high", "rank": 1,
                "score_bp": 7200,
                "components_bp": {"change": 8000, "opera": 10000, "road": 4000},
                "metrics": {
                    "valid_bp": 9500, "decrease_bp": 2200, "increase_bp": 100,
                    "changed_bp": 2300, "p90_abs_delta_cdb": 740,
                    "opera_status": "corroborated", "opera_overlap_bp": 6000,
                    "road_m_near_change": 900, "road_m_total": 1200
                },
                "explanation": ["Backscatter dropped."]
            }),
        )
    }

    fn action(parents: Vec<String>) -> Block {
        seal(
            "action",
            parents,
            json!({
                "cell_id": "piura-demo-r02c01", "rank": 1,
                "action": "field_verification_first", "audience": "GORE Piura"
            }),
        )
    }

    #[test]
    fn full_chain_accepted() {
        let (ch, cx) = (change(), context());
        let rk = risk(vec![ch.id.clone(), cx.id.clone()]);
        let ac = action(vec![rk.id.clone()]);
        verify_chain(&[ch, cx, rk, ac]).unwrap();
    }

    #[test]
    fn risk_with_a_parent_that_does_not_exist() {
        let cx = context();
        let missing = format!("sha256:{}", "11".repeat(32));
        let rk = risk(vec![missing.clone(), cx.id.clone()]);
        let err = verify_chain(&[cx, rk]).unwrap_err();
        match err {
            WonbError::MissingParent { parent, .. } => assert_eq!(parent, missing),
            other => panic!("se esperaba MissingParent, llegó {other}"),
        }
    }

    #[test]
    fn risk_needs_both_a_change_and_a_context() {
        let ch = change();
        let rk = risk(vec![ch.id.clone(), ch.id.clone()]);
        let err = verify_chain(&[ch, rk]).unwrap_err();
        assert!(matches!(err, WonbError::Schema(_)), "{err}");
    }

    #[test]
    fn action_cannot_hang_from_a_change() {
        let ch = change();
        let ac = action(vec![ch.id.clone()]);
        let err = verify_chain(&[ch, ac]).unwrap_err();
        assert!(matches!(err, WonbError::Schema(_)), "{err}");
    }

    #[test]
    fn change_cannot_have_parents() {
        let cx = context();
        let mut raw = serde_json::to_value(change()).unwrap();
        raw["parents"] = json!([cx.id.clone()]);
        // El `id` deja de cuadrar, pero verify_chain solo mira la topología:
        // así se distingue un fallo de cadena de un fallo de firma.
        let ch = Block::from_value(raw).unwrap();
        let err = verify_chain(&[cx, ch]).unwrap_err();
        assert!(matches!(err, WonbError::Schema(_)), "{err}");
    }
}
