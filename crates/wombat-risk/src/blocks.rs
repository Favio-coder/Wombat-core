//! De celdas puntuadas a bloques WONB 0.2.
//!
//! Aquí se arma la cadena Change/Context → Risk → Action. Los tipos son los de
//! [`wombat_core::v02`]: este crate no define su propio envelope ni su propio
//! payload, porque dos definiciones del mismo contrato acaban divergiendo y el
//! día que divergen nadie puede verificar nada.
//!
//! Sigue sin haber reloj: `issued_at` y los padres entran por [`BlockContext`],
//! los pone quien exporta el bundle. Los `valid_until` salen del TTL de la
//! configuración sumado a ese `issued_at`.

use wombat_core::v02::{
    ActionKind, ActionPayload, BlockBody, BlockType, Payload, Provenance, RiskPayload,
};

use crate::config::RiskConfig;
use crate::error::RiskError;
use crate::evaluate::CellRisk;

/// Lo que un bloque necesita y la regla de riesgo no sabe: dónde, cuándo, quién
/// y de qué cuelga.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockContext {
    pub aoi_id: String,
    /// Prefijo del topic, p. ej. `wombat/pe/piura`. El tipo se añade al final.
    pub topic_prefix: String,
    /// RFC 3339, tal como se va a firmar. No se reformatea.
    pub issued_at: String,
    /// `ed25519:<64 hex>` de quien firmará.
    pub issuer: String,
    /// `id` del bloque Change del que cuelgan los Risk.
    pub change_id: String,
    /// `id` del bloque Context del que cuelgan los Risk.
    pub context_id: String,
    pub provenance: Provenance,
}

impl BlockContext {
    fn topic(&self, block_type: BlockType) -> String {
        format!("{}/{}/{}", self.topic_prefix, self.aoi_id, block_type)
    }
}

/// Payloads Risk, uno por celda, en el orden del ranking.
///
/// Son solo los payloads: sirven para compararlos contra un bundle existente
/// sin que el emisor ni el momento de emisión entren en juego.
pub fn build_risk_payloads(
    _cfg: &RiskConfig,
    cells: &[CellRisk],
) -> Result<Vec<RiskPayload>, RiskError> {
    Ok(cells.iter().map(risk_payload).collect())
}

/// Payloads Action: solo el top `top_n_actions` de la configuración, y solo
/// celdas que pasaron el gate de calidad.
///
/// Pedirle a alguien que vaya al campo por una celda sin datos suficientes
/// sería gastarle el viaje.
pub fn build_action_payloads(
    cfg: &RiskConfig,
    cells: &[CellRisk],
) -> Result<Vec<ActionPayload>, RiskError> {
    let mut ranked: Vec<&CellRisk> = cells.iter().filter(|c| c.is_ranked()).collect();
    ranked.sort_by_key(|c| c.rank);
    Ok(ranked
        .into_iter()
        .take(cfg.top_n_actions)
        .map(|cell| ActionPayload {
            cell_id: cell.cell_id.clone(),
            rank: cell.rank.unwrap_or_default(),
            action: ActionKind::FieldVerificationFirst,
            audience: AUDIENCE.to_string(),
        })
        .collect())
}

/// Cuerpos de bloque Risk, listos para `Block::seal`.
///
/// Cada uno cuelga del Change y del Context, en ese orden, como exige el
/// esquema (`parents` de exactamente dos elementos).
pub fn build_risk_bodies(
    cfg: &RiskConfig,
    cells: &[CellRisk],
    ctx: &BlockContext,
) -> Result<Vec<BlockBody>, RiskError> {
    let valid_until = expires(ctx, cfg.ttl_seconds.risk)?;
    cells
        .iter()
        .map(|cell| {
            Ok(BlockBody {
                wonb_version: wombat_core::v02::WONB_VERSION.to_string(),
                block_type: BlockType::Risk,
                topic: ctx.topic(BlockType::Risk),
                issued_at: ctx.issued_at.clone(),
                valid_until: Some(valid_until.clone()),
                issuer: ctx.issuer.clone(),
                parents: vec![ctx.change_id.clone(), ctx.context_id.clone()],
                payload: Payload::Risk(risk_payload(cell)),
                provenance: ctx.provenance.clone(),
            })
        })
        .collect()
}

/// Cuerpos de bloque Action, uno por cada Risk del top `top_n_actions`.
///
/// Toma los bloques Risk **ya sellados** porque el padre de un Action es el
/// `id` de su Risk, y ese `id` es el hash de su cuerpo: no existe hasta que el
/// Risk está sellado.
pub fn build_action_bodies(
    cfg: &RiskConfig,
    risk_blocks: &[wombat_core::v02::Block],
    ctx: &BlockContext,
) -> Result<Vec<BlockBody>, RiskError> {
    let valid_until = expires(ctx, cfg.ttl_seconds.action)?;

    let mut candidates: Vec<(u32, &str, &str)> = Vec::new();
    for block in risk_blocks {
        let Payload::Risk(payload) = &block.body.payload else {
            return Err(RiskError::Input(format!(
                "se esperaba un bloque risk, llegó {}",
                block.body.block_type
            )));
        };
        if let Some(rank) = payload.rank {
            candidates.push((rank, payload.cell_id.as_str(), block.id.as_str()));
        }
    }
    candidates.sort_unstable();

    Ok(candidates
        .into_iter()
        .take(cfg.top_n_actions)
        .map(|(rank, cell_id, risk_id)| BlockBody {
            wonb_version: wombat_core::v02::WONB_VERSION.to_string(),
            block_type: BlockType::Action,
            topic: ctx.topic(BlockType::Action),
            issued_at: ctx.issued_at.clone(),
            valid_until: Some(valid_until.clone()),
            issuer: ctx.issuer.clone(),
            parents: vec![risk_id.to_string()],
            payload: Payload::Action(ActionPayload {
                cell_id: cell_id.to_string(),
                rank,
                action: ActionKind::FieldVerificationFirst,
                audience: AUDIENCE.to_string(),
            }),
            provenance: ctx.provenance.clone(),
        })
        .collect())
}

/// A quién va dirigida la acción. Igual que en el bundle fixture.
const AUDIENCE: &str = "GORE Piura risk analyst";

fn risk_payload(cell: &CellRisk) -> RiskPayload {
    RiskPayload {
        cell_id: cell.cell_id.clone(),
        status: cell.status,
        rank: cell.rank,
        score_bp: cell.score_bp,
        components_bp: cell.components_bp.clone(),
        metrics: cell.metrics.clone(),
        explanation: cell.explanation.clone(),
    }
}

/// `issued_at + ttl`, con el formato que fija wombat-core.
///
/// El TTL sale de la configuración, no del tipo de bloque: si algún día el
/// contrato cambia la vida útil de un Risk, se cambia en `risk-config.json` y
/// no hay que recompilar nada.
fn expires(ctx: &BlockContext, ttl_seconds: u64) -> Result<String, RiskError> {
    Ok(wombat_core::v02::valid_until_from(
        &ctx.issued_at,
        ttl_seconds,
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::tests::config;
    use crate::evaluate::evaluate;
    use crate::inputs::Inputs;
    use wombat_core::v02::{generate_ephemeral_keypair, issuer_string, Block, Source};

    /// Cinco celdas; el score lo decide `road_m_near_change`, y la última no
    /// pasa el gate de calidad.
    fn inputs() -> Inputs {
        let features: Vec<String> = [
            ("piura-demo-r00c00", "0.9", 100),
            ("piura-demo-r00c01", "0.9", 700),
            ("piura-demo-r00c02", "0.9", 400),
            ("piura-demo-r00c03", "0.9", 900),
            ("piura-demo-r01c00", "0.55", 300),
        ]
        .iter()
        .map(|(cell_id, valid, road)| {
            format!(
                r#"{{"type":"Feature","geometry":{{"type":"Polygon","coordinates":[[[0,0]]]}},
                   "properties":{{"cell_id":"{cell_id}","valid_fraction":{valid},
                   "decrease_fraction":0.05,"increase_fraction":0.05,"changed_fraction":0.1,
                   "p90_abs_delta_db":4.0,"opera_status":"corroborated",
                   "opera_overlap_ratio":0.3,"road_m_near_change":{road},
                   "road_m_total":5000}}}}"#
            )
        })
        .collect();
        Inputs::from_cells_geojson(
            "piura-demo",
            &format!(
                r#"{{"type":"FeatureCollection","features":[{}]}}"#,
                features.join(",")
            ),
        )
        .unwrap()
    }

    fn context(issuer: String) -> BlockContext {
        BlockContext {
            aoi_id: "piura-demo".into(),
            topic_prefix: "wombat/pe/piura".into(),
            issued_at: "2026-11-14T17:00:00Z".into(),
            issuer,
            change_id: format!("sha256:{}", "aa".repeat(32)),
            context_id: format!("sha256:{}", "bb".repeat(32)),
            provenance: Provenance {
                sources: vec![Source {
                    product: "NISAR_L2_GCOV".into(),
                    extra: Default::default(),
                }],
                method: "risk-config/0.1".into(),
                code_commit: "test".into(),
            },
        }
    }

    #[test]
    fn actions_only_for_the_top_n_that_passed_the_gate() {
        let cfg = config();
        let cells = evaluate(&cfg, &inputs()).unwrap();
        let actions = build_action_payloads(&cfg, &cells).unwrap();

        assert_eq!(actions.len(), cfg.top_n_actions);
        assert_eq!(
            actions.iter().map(|a| a.rank).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        // La celda del gate no recibe acción aunque tenga carretera cerca:
        // sería gastarle el viaje a alguien.
        assert!(!actions.iter().any(|a| a.cell_id.ends_with("r01c00")));
        assert_eq!(
            actions[0].cell_id, "piura-demo-r00c03",
            "la de más carretera"
        );
    }

    #[test]
    fn a_top_n_larger_than_the_ranking_does_not_invent_actions() {
        let cfg = RiskConfig::from_json(
            &crate::config::tests::config_json()
                .replace("\"top_n_actions\": 3", "\"top_n_actions\": 99"),
        )
        .unwrap();
        let cells = evaluate(&cfg, &inputs()).unwrap();
        let actions = build_action_payloads(&cfg, &cells).unwrap();
        assert_eq!(actions.len(), 4, "4 celdas pasaron el gate, no 5");
    }

    #[test]
    fn risk_bodies_carry_both_parents_and_the_config_ttl() {
        let cfg = config();
        let cells = evaluate(&cfg, &inputs()).unwrap();
        let ctx = context("ed25519:00".into());
        let bodies = build_risk_bodies(&cfg, &cells, &ctx).unwrap();

        assert_eq!(bodies.len(), cells.len());
        for body in &bodies {
            assert_eq!(body.block_type, BlockType::Risk);
            assert_eq!(body.topic, "wombat/pe/piura/piura-demo/risk");
            assert_eq!(
                body.parents,
                vec![ctx.change_id.clone(), ctx.context_id.clone()]
            );
            // 86400 s después del issued_at, según el TTL de la config.
            assert_eq!(body.valid_until.as_deref(), Some("2026-11-15T17:00:00Z"));
        }
    }

    #[test]
    fn sealed_blocks_form_a_chain_that_wombat_core_accepts() {
        let cfg = config();
        let cells = evaluate(&cfg, &inputs()).unwrap();
        let (key, vk) = generate_ephemeral_keypair();
        let mut ctx = context(issuer_string(&vk));

        // Un Change y un Context reales, para que los padres existan de verdad.
        let change = Block::seal(change_body(&ctx), &key).unwrap();
        let context_block = Block::seal(context_body(&ctx), &key).unwrap();
        ctx.change_id = change.id.clone();
        ctx.context_id = context_block.id.clone();

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

        assert_eq!(actions.len(), cfg.top_n_actions);
        let mut all = vec![change, context_block];
        all.extend(risks);
        all.extend(actions);
        wombat_core::v02::verify_chain(&all).unwrap();
    }

    #[test]
    fn action_bodies_hang_from_their_own_risk_block() {
        let cfg = config();
        let cells = evaluate(&cfg, &inputs()).unwrap();
        let (key, vk) = generate_ephemeral_keypair();
        let ctx = context(issuer_string(&vk));
        let risks: Vec<Block> = build_risk_bodies(&cfg, &cells, &ctx)
            .unwrap()
            .into_iter()
            .map(|body| Block::seal(body, &key).unwrap())
            .collect();
        let actions = build_action_bodies(&cfg, &risks, &ctx).unwrap();

        for action in &actions {
            let Payload::Action(payload) = &action.payload else {
                panic!("se esperaba un payload action")
            };
            let parent = risks.iter().find(|r| r.id == action.parents[0]).unwrap();
            let Payload::Risk(parent_payload) = &parent.body.payload else {
                panic!("se esperaba un payload risk")
            };
            assert_eq!(parent_payload.cell_id, payload.cell_id);
            assert_eq!(parent_payload.rank, Some(payload.rank));
            assert_eq!(action.valid_until.as_deref(), Some("2026-11-14T18:00:00Z"));
        }
    }

    #[test]
    fn build_action_bodies_rejects_a_block_that_is_not_a_risk() {
        let cfg = config();
        let (key, vk) = generate_ephemeral_keypair();
        let ctx = context(issuer_string(&vk));
        let change = Block::seal(change_body(&ctx), &key).unwrap();
        let err = build_action_bodies(&cfg, &[change], &ctx).unwrap_err();
        assert!(matches!(err, RiskError::Input(_)), "{err}");
    }

    fn change_body(ctx: &BlockContext) -> BlockBody {
        use wombat_core::v02::{ChangePayload, DataOrigin, Frequency, OverlayHashes};
        BlockBody {
            wonb_version: wombat_core::v02::WONB_VERSION.to_string(),
            block_type: BlockType::Change,
            topic: ctx.topic(BlockType::Change),
            issued_at: ctx.issued_at.clone(),
            valid_until: None,
            issuer: ctx.issuer.clone(),
            parents: Vec::new(),
            payload: Payload::Change(ChangePayload {
                aoi_id: ctx.aoi_id.clone(),
                data_origin: DataOrigin::Simulated,
                t1: "2026-10-14T23:00:00Z".into(),
                t2: "2026-10-26T23:00:00Z".into(),
                polarization: "HHHH".into(),
                frequency: Frequency::A,
                pixel_spacing_m: 20,
                delta_db_threshold_cdb: 300,
                aoi_valid_bp: 9000,
                aoi_decrease_bp: 500,
                aoi_increase_bp: 500,
                overlay_sha256: OverlayHashes {
                    t1_png: "11".repeat(32),
                    t2_png: "22".repeat(32),
                    delta_png: "33".repeat(32),
                },
                cells_geojson_sha256: "44".repeat(32),
            }),
            provenance: ctx.provenance.clone(),
        }
    }

    fn context_body(ctx: &BlockContext) -> BlockBody {
        use wombat_core::v02::ContextPayload;
        BlockBody {
            wonb_version: wombat_core::v02::WONB_VERSION.to_string(),
            block_type: BlockType::Context,
            topic: ctx.topic(BlockType::Context),
            issued_at: ctx.issued_at.clone(),
            valid_until: None,
            issuer: ctx.issuer.clone(),
            parents: Vec::new(),
            payload: Payload::Context(ContextPayload {
                aoi_id: ctx.aoi_id.clone(),
                roads_total_m: 16242,
                road_buffer_m: 250,
                osm_retrieved_at: "2026-11-14T15:00:00Z".into(),
                roads_geojson_sha256: "55".repeat(32),
            }),
            provenance: ctx.provenance.clone(),
        }
    }
}
