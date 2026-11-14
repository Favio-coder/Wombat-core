//! La regla de riesgo (plan maestro §8.3), en aritmética exacta.
//!
//! ```text
//! change_c = min(changed_fraction / change_saturation, 1)
//! opera_c  = opera_overlap_ratio, o 0 si el estado OPERA es unavailable
//! road_c   = road_m_near_change / max(road_m_near_change entre celdas que pasan el gate)
//! score    = w_change*change_c + w_opera*opera_c + w_road*road_c
//! ```
//!
//! Dos detalles que no se pueden cambiar sin romper el bundle publicado:
//!
//! - **`score_bp` se redondea una sola vez, al final.** Es
//!   `bp(w_c·c + w_o·o + w_r·r)` con los componentes **sin redondear**, no la
//!   suma ponderada de `change_bp`, `opera_bp` y `road_bp`. Redondear antes
//!   mueve varias celdas uno o dos bp y con ellas su rank.
//! - **El gate de calidad se aplica antes de buscar el máximo de carretera.**
//!   Una celda sin datos suficientes no debe estirar el denominador de
//!   `road_c` para todas las demás.
//!
//! Esta función es pura: sin reloj, sin azar, sin red, sin I/O.

use std::cmp::Ordering;

use wombat_core::v02::{ComponentsBp, OperaStatus, RiskMetrics, RiskStatus};

use crate::config::RiskConfig;
use crate::error::RiskError;
use crate::exact::Exact;
use crate::explain;
use crate::inputs::{CellInput, Inputs};

/// Una celda ya puntuada, con todo lo que necesita su bloque Risk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellRisk {
    pub cell_id: String,
    pub status: RiskStatus,
    /// `None` cuando la celda no pasa el gate de calidad.
    pub rank: Option<u32>,
    pub score_bp: Option<u32>,
    pub components_bp: Option<ComponentsBp>,
    pub metrics: RiskMetrics,
    pub explanation: Vec<String>,
}

impl CellRisk {
    /// Si la celda entró al ranking.
    pub fn is_ranked(&self) -> bool {
        self.rank.is_some()
    }
}

/// Puntúa y ordena todas las celdas.
///
/// El orden de salida **es** el ranking: `score_bp` desc, luego `change_bp`
/// desc, luego `cell_id` asc. Las celdas que no pasan el gate van al final,
/// ordenadas por `cell_id`, y reciben su bloque Risk con `status:
/// insufficient_quality` para que el mapa pueda decir "sin datos suficientes".
pub fn evaluate(cfg: &RiskConfig, inputs: &Inputs) -> Result<Vec<CellRisk>, RiskError> {
    let gate = Exact::new(i128::from(cfg.quality_min_valid_fraction_bp), 10_000)?;
    let passes_gate = |cell: &CellInput| cell.valid_fraction.cmp_exact(&gate) != Ordering::Less;

    // El denominador de road_c sale solo de las celdas que pasan el gate.
    let max_road = inputs
        .cells
        .iter()
        .filter(|c| passes_gate(c))
        .map(|c| c.road_m_near_change)
        .max()
        .unwrap_or(0);

    let mut ranked = Vec::new();
    let mut rejected = Vec::new();
    for cell in &inputs.cells {
        if passes_gate(cell) {
            ranked.push(score(cfg, cell, max_road)?);
        } else {
            rejected.push(insufficient_quality(cfg, cell)?);
        }
    }

    // Desempate completo y determinista: sin él, dos celdas con el mismo score
    // podrían alternar de rank entre ejecuciones y el "top 3" dejaría de ser
    // reproducible.
    ranked.sort_by(|a, b| {
        b.score_bp
            .cmp(&a.score_bp)
            .then_with(|| b.change_bp.cmp(&a.change_bp))
            .then_with(|| a.cell_id.cmp(&b.cell_id))
    });
    rejected.sort_by(|a, b| a.cell_id.cmp(&b.cell_id));

    let mut out = Vec::with_capacity(ranked.len() + rejected.len());
    for (index, scored) in ranked.into_iter().enumerate() {
        let rank = u32::try_from(index + 1)
            .map_err(|_| RiskError::Arithmetic("demasiadas celdas para un rank u32".into()))?;
        out.push(scored.into_cell_risk(cfg, rank)?);
    }
    out.extend(rejected);
    Ok(out)
}

/// Una celda puntuada, antes de conocer su rank.
struct Scored<'a> {
    cell: &'a CellInput,
    cell_id: String,
    score_bp: u32,
    change_bp: u32,
    opera_bp: u32,
    road_bp: u32,
}

impl Scored<'_> {
    fn into_cell_risk(self, cfg: &RiskConfig, rank: u32) -> Result<CellRisk, RiskError> {
        let thresholds = cfg.status_thresholds_bp;
        let status = if self.score_bp >= thresholds.review_high {
            RiskStatus::ReviewHigh
        } else if self.score_bp >= thresholds.review_medium {
            RiskStatus::ReviewMedium
        } else {
            RiskStatus::ReviewLow
        };
        Ok(CellRisk {
            status,
            rank: Some(rank),
            score_bp: Some(self.score_bp),
            components_bp: Some(ComponentsBp {
                change: self.change_bp,
                opera: self.opera_bp,
                road: self.road_bp,
            }),
            metrics: metrics(self.cell)?,
            explanation: explain::ranked(cfg, self.cell)?,
            cell_id: self.cell_id,
        })
    }
}

fn score<'a>(
    cfg: &RiskConfig,
    cell: &'a CellInput,
    max_road: u32,
) -> Result<Scored<'a>, RiskError> {
    let change_c = cell
        .changed_fraction
        .div(cfg.change_saturation)?
        .min(Exact::ONE);

    let opera_c = match (cell.opera_status, cell.opera_overlap_ratio) {
        // Sin observación independiente el componente es 0, no "lo que había":
        // la ausencia de contraste no corrobora nada.
        (OperaStatus::Unavailable, _) | (_, None) => Exact::ZERO,
        (_, Some(overlap)) => overlap,
    };

    // Ninguna celda con carretera cerca: el componente es 0 para todas, sin
    // dividir por cero.
    let road_c = if max_road == 0 {
        Exact::ZERO
    } else {
        Exact::new(i128::from(cell.road_m_near_change), i128::from(max_road))?
    };

    let weight = |bp: u32| Exact::new(i128::from(bp), 10_000);
    let weighted = weight(cfg.weights_bp.change)?
        .mul(change_c)?
        .add(weight(cfg.weights_bp.opera)?.mul(opera_c)?)?
        .add(weight(cfg.weights_bp.road)?.mul(road_c)?)?;

    Ok(Scored {
        cell,
        cell_id: cell.cell_id.clone(),
        // Un solo redondeo, sobre la suma sin redondear.
        score_bp: weighted.to_bp_u32()?,
        change_bp: change_c.to_bp_u32()?,
        opera_bp: opera_c.to_bp_u32()?,
        road_bp: road_c.to_bp_u32()?,
    })
}

fn insufficient_quality(cfg: &RiskConfig, cell: &CellInput) -> Result<CellRisk, RiskError> {
    Ok(CellRisk {
        cell_id: cell.cell_id.clone(),
        status: RiskStatus::InsufficientQuality,
        rank: None,
        score_bp: None,
        components_bp: None,
        metrics: metrics(cell)?,
        explanation: explain::insufficient_quality(cfg, cell)?,
    })
}

/// Métricas crudas de la celda, en las unidades enteras del contrato.
fn metrics(cell: &CellInput) -> Result<RiskMetrics, RiskError> {
    Ok(RiskMetrics {
        valid_bp: cell.valid_fraction.to_bp_u32()?,
        decrease_bp: cell.decrease_fraction.to_bp_u32()?,
        increase_bp: cell.increase_fraction.to_bp_u32()?,
        changed_bp: cell.changed_fraction.to_bp_u32()?,
        // dB → centi-dB: bp(db/100), la misma conversión del fixture.
        p90_abs_delta_cdb: cell
            .p90_abs_delta_db
            .map(|db| db.div(Exact::int(100))?.to_bp_u32())
            .transpose()?,
        opera_status: cell.opera_status,
        opera_overlap_bp: cell.opera_overlap_ratio.map(Exact::to_bp_u32).transpose()?,
        road_m_near_change: cell.road_m_near_change,
        road_m_total: cell.road_m_total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::tests::{config, config_json};
    use crate::inputs::tests::{one_cell_geojson, props};

    /// Celdas de prueba con el score controlado por `road_m_near_change`.
    fn inputs_with(cells: &[(&str, &str, u32)]) -> Inputs {
        let features: Vec<String> = cells
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
        let json = format!(
            r#"{{"type":"FeatureCollection","features":[{}]}}"#,
            features.join(",")
        );
        Inputs::from_cells_geojson("piura-demo", &json).unwrap()
    }

    #[test]
    fn score_is_rounded_once_at_the_end() {
        // change_c = 0.1/0.25 = 0.4, opera = 0.3, road = 1
        // score = 0.45*0.4 + 0.35*0.3 + 0.20*1 = 0.485 -> 4850 bp
        let cells = evaluate(
            &config(),
            &inputs_with(&[("piura-demo-r00c00", "0.9", 100)]),
        )
        .unwrap();
        assert_eq!(cells[0].score_bp, Some(4850));
        let components = cells[0].components_bp.as_ref().unwrap();
        assert_eq!(
            (components.change, components.opera, components.road),
            (4000, 3000, 10000)
        );
        assert_eq!(cells[0].status, RiskStatus::ReviewMedium);
    }

    #[test]
    fn ties_break_by_cell_id_ascending() {
        // Mismo score y mismo change_bp: solo el cell_id decide.
        let cells = evaluate(
            &config(),
            &inputs_with(&[
                ("piura-demo-r09c09", "0.9", 500),
                ("piura-demo-r00c01", "0.9", 500),
                ("piura-demo-r00c00", "0.9", 500),
            ]),
        )
        .unwrap();
        let ids: Vec<&str> = cells.iter().map(|c| c.cell_id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "piura-demo-r00c00",
                "piura-demo-r00c01",
                "piura-demo-r09c09"
            ]
        );
        assert_eq!(cells[0].rank, Some(1));
        assert_eq!(cells[2].rank, Some(3));
        let scores: Vec<Option<u32>> = cells.iter().map(|c| c.score_bp).collect();
        assert_eq!(scores[0], scores[1], "el empate era real");
    }

    #[test]
    fn the_quality_gate_excludes_a_cell_without_ranking_it() {
        let cells = evaluate(
            &config(),
            &inputs_with(&[
                ("piura-demo-r00c00", "0.9", 100),
                ("piura-demo-r00c01", "0.55", 900),
            ]),
        )
        .unwrap();
        let rejected = cells.iter().find(|c| c.cell_id.ends_with("c01")).unwrap();
        assert_eq!(rejected.status, RiskStatus::InsufficientQuality);
        assert_eq!(rejected.rank, None);
        assert_eq!(rejected.score_bp, None);
        assert_eq!(rejected.components_bp, None);
        // Pero sigue teniendo métricas y su bloque: el mapa necesita decirlo.
        assert_eq!(rejected.metrics.valid_bp, 5500);
        assert_eq!(rejected.explanation.len(), 1);
    }

    #[test]
    fn a_rejected_cell_does_not_stretch_the_road_denominator() {
        // La celda mala tiene 900 m, el doble que la buena. Si entrara al
        // denominador, el road_c de la buena bajaría de 1 a 0.5.
        let cells = evaluate(
            &config(),
            &inputs_with(&[
                ("piura-demo-r00c00", "0.9", 450),
                ("piura-demo-r00c01", "0.55", 900),
            ]),
        )
        .unwrap();
        let good = cells.iter().find(|c| c.cell_id.ends_with("c00")).unwrap();
        assert_eq!(good.components_bp.as_ref().unwrap().road, 10_000);
    }

    #[test]
    fn no_road_anywhere_gives_zero_without_dividing_by_zero() {
        let cells = evaluate(
            &config(),
            &inputs_with(&[
                ("piura-demo-r00c00", "0.9", 0),
                ("piura-demo-r00c01", "0.9", 0),
            ]),
        )
        .unwrap();
        for cell in &cells {
            assert_eq!(cell.components_bp.as_ref().unwrap().road, 0);
            // score = 0.45*0.4 + 0.35*0.3 = 0.285
            assert_eq!(cell.score_bp, Some(2850));
        }
    }

    #[test]
    fn unavailable_opera_contributes_zero() {
        let json = one_cell_geojson(
            r#"{"cell_id":"piura-demo-r00c00","valid_fraction":0.9,"decrease_fraction":0.05,
                "increase_fraction":0.05,"changed_fraction":0.1,"p90_abs_delta_db":4.0,
                "opera_status":"unavailable","opera_overlap_ratio":null,
                "road_m_near_change":100,"road_m_total":5000}"#,
        );
        let inputs = Inputs::from_cells_geojson("piura-demo", &json).unwrap();
        let cells = evaluate(&config(), &inputs).unwrap();
        assert_eq!(cells[0].components_bp.as_ref().unwrap().opera, 0);
        assert_eq!(cells[0].metrics.opera_overlap_bp, None);
        // score = 0.45*0.4 + 0.20*1 = 0.38
        assert_eq!(cells[0].score_bp, Some(3800));
    }

    #[test]
    fn change_component_saturates_at_one() {
        let json = one_cell_geojson(&props("piura-demo-r00c00", "0.9", "0.5", "0.3"));
        let inputs = Inputs::from_cells_geojson("piura-demo", &json).unwrap();
        let cells = evaluate(&config(), &inputs).unwrap();
        // changed = 0.8, /0.25 = 3.2, saturado a 1.
        assert_eq!(cells[0].components_bp.as_ref().unwrap().change, 10_000);
    }

    #[test]
    fn changing_a_weight_changes_the_ranking() {
        // Prueba que ningún peso está escrito en el código: la celda con mucha
        // carretera y poco cambio gana solo si road pesa más.
        let inputs = Inputs::from_cells_geojson(
            "piura-demo",
            &format!(
                r#"{{"type":"FeatureCollection","features":[
                  {{"type":"Feature","geometry":{{"type":"Polygon","coordinates":[[[0,0]]]}},
                   "properties":{}}},
                  {{"type":"Feature","geometry":{{"type":"Polygon","coordinates":[[[0,0]]]}},
                   "properties":{{"cell_id":"piura-demo-r00c01","valid_fraction":0.9,
                   "decrease_fraction":0.01,"increase_fraction":0.0,"changed_fraction":0.01,
                   "p90_abs_delta_db":3.1,"opera_status":"not_corroborated",
                   "opera_overlap_ratio":0.0,"road_m_near_change":1000,"road_m_total":5000}}}}]}}"#,
                props("piura-demo-r00c00", "0.9", "0.2", "0.0")
            ),
        )
        .unwrap();

        let by_change = evaluate(&config(), &inputs).unwrap();
        assert_eq!(by_change[0].cell_id, "piura-demo-r00c00");

        let road_heavy = RiskConfig::from_json(&config_json().replace(
            r#""weights": { "change": 0.45, "opera": 0.35, "road": 0.20 }"#,
            r#""weights": { "change": 0.05, "opera": 0.05, "road": 0.90 }"#,
        ))
        .unwrap();
        let by_road = evaluate(&road_heavy, &inputs).unwrap();
        assert_eq!(by_road[0].cell_id, "piura-demo-r00c01");
    }

    #[test]
    fn changing_a_status_threshold_changes_the_status() {
        let inputs = inputs_with(&[("piura-demo-r00c00", "0.9", 100)]);
        assert_eq!(
            evaluate(&config(), &inputs).unwrap()[0].status,
            RiskStatus::ReviewMedium
        );
        let strict = RiskConfig::from_json(&config_json().replace(
            r#""status_thresholds": { "review_high": 0.60, "review_medium": 0.35 }"#,
            r#""status_thresholds": { "review_high": 0.40, "review_medium": 0.10 }"#,
        ))
        .unwrap();
        assert_eq!(
            evaluate(&strict, &inputs).unwrap()[0].status,
            RiskStatus::ReviewHigh
        );
    }

    #[test]
    fn two_runs_are_identical() {
        let inputs = inputs_with(&[
            ("piura-demo-r00c00", "0.9", 100),
            ("piura-demo-r00c01", "0.9", 700),
            ("piura-demo-r00c02", "0.55", 900),
        ]);
        let cfg = config();
        assert_eq!(
            evaluate(&cfg, &inputs).unwrap(),
            evaluate(&cfg, &inputs).unwrap()
        );
    }
}
