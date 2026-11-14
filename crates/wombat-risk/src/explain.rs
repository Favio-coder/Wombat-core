//! Las frases que acompañan a cada celda.
//!
//! Son parte del payload firmado, así que son parte del contrato: estas cadenas
//! se reproducen **byte a byte** con las del bundle fixture. Un cambio de
//! redacción cambia el hash de todos los bloques Risk, y por eso vive aquí y no
//! en el dashboard (R20, R26: el dashboard no importa código del core).
//!
//! Están en inglés porque así están en `contracts/v1`: el público son jueces
//! globales. Los números se redondean con la misma regla que el fixture,
//! `(bp + 50) / 100`, no con la del idioma de turno.

use wombat_core::v02::OperaStatus;

use crate::config::RiskConfig;
use crate::error::RiskError;
use crate::exact::{bp_to_percent, Exact};
use crate::inputs::CellInput;

/// Las tres frases de una celda que entró al ranking: qué cambió, si alguien
/// más lo vio, y qué hay cerca que importe.
pub fn ranked(cfg: &RiskConfig, cell: &CellInput) -> Result<Vec<String>, RiskError> {
    let threshold_db = cfg.delta_db_threshold.trimmed_decimal()?;
    let decrease = bp_to_percent(cell.decrease_fraction.to_bp_u32()?);
    let increase = bp_to_percent(cell.increase_fraction.to_bp_u32()?);

    let mut out = vec![format!(
        "{decrease}% of valid pixels decreased \u{2265}{threshold_db} dB; \
         {increase}% increased \u{2265}{threshold_db} dB."
    )];
    out.push(opera_sentence(cfg, cell)?);
    out.push(format!(
        "{} km of road lie within {} m of changed pixels.",
        Exact::new(i128::from(cell.road_m_near_change), 1000)?.round_half_even_str(1)?,
        cfg.road_buffer_meters
    ));
    Ok(out)
}

/// La única frase de una celda que no pasó el gate de calidad.
///
/// Dice por qué queda fuera, en lugar de dejar un hueco en el mapa.
pub fn insufficient_quality(cfg: &RiskConfig, cell: &CellInput) -> Result<Vec<String>, RiskError> {
    Ok(vec![format!(
        "Only {}% of pixels are valid (minimum {}%); excluded from the ranking.",
        bp_to_percent(cell.valid_fraction.to_bp_u32()?),
        bp_to_percent(cfg.quality_min_valid_fraction_bp)
    )])
}

/// Qué dice la fuente independiente. Sin ella, se dice que no la hay.
fn opera_sentence(cfg: &RiskConfig, cell: &CellInput) -> Result<String, RiskError> {
    if cell.opera_status == OperaStatus::Unavailable {
        return Ok(format!(
            "No OPERA DSWx-S1 (Sentinel-1) observation within {} days; \
             no independent contrast available.",
            cfg.opera_max_gap_days
        ));
    }
    let overlap = match cell.opera_overlap_ratio {
        Some(ratio) => bp_to_percent(ratio.to_bp_u32()?),
        // `inputs` ya rechaza esta combinación; aquí solo se evita inventar.
        None => {
            return Err(RiskError::Input(format!(
                "{}: opera_status {:?} sin solapamiento",
                cell.cell_id, cell.opera_status
            )))
        }
    };
    let threshold = bp_to_percent(cfg.corroboration_overlap_threshold_bp);
    Ok(match cell.opera_status {
        OperaStatus::Corroborated => format!(
            "OPERA DSWx-S1 (Sentinel-1) shows new water on {overlap}% of changed area \
             (threshold {threshold}%)."
        ),
        _ => format!(
            "OPERA DSWx-S1 (Sentinel-1) shows new water on only {overlap}% of changed area \
             (below the {threshold}% threshold)."
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::tests::config;
    use crate::inputs::{tests::one_cell_geojson, Inputs};

    fn cell(props: &str) -> CellInput {
        Inputs::from_cells_geojson("piura-demo", &one_cell_geojson(props))
            .unwrap()
            .cells
            .remove(0)
    }

    /// Celda r00c00 del fixture: 2% decrece, 1% crece, OPERA no corrobora.
    #[test]
    fn wording_matches_the_fixture_for_a_not_corroborated_cell() {
        let c = cell(
            r#"{"cell_id":"piura-demo-r00c00","valid_fraction":0.95,"decrease_fraction":0.02,
                "increase_fraction":0.01,"changed_fraction":0.03,"p90_abs_delta_db":3.3,
                "opera_status":"not_corroborated","opera_overlap_ratio":0.05,
                "road_m_near_change":300,"road_m_total":2100}"#,
        );
        assert_eq!(
            ranked(&config(), &c).unwrap(),
            vec![
                "2% of valid pixels decreased \u{2265}3 dB; 1% increased \u{2265}3 dB.",
                "OPERA DSWx-S1 (Sentinel-1) shows new water on only 5% of changed area (below the 20% threshold).",
                "0.3 km of road lie within 250 m of changed pixels.",
            ]
        );
    }

    /// Celda r02c01 del fixture: la de rank 1.
    #[test]
    fn wording_matches_the_fixture_for_a_corroborated_cell() {
        let c = cell(
            r#"{"cell_id":"piura-demo-r02c01","valid_fraction":0.89,"decrease_fraction":0.21,
                "increase_fraction":0.05,"changed_fraction":0.26,"p90_abs_delta_db":5.6,
                "opera_status":"corroborated","opera_overlap_ratio":0.38,
                "road_m_near_change":2800,"road_m_total":5600}"#,
        );
        assert_eq!(
            ranked(&config(), &c).unwrap(),
            vec![
                "21% of valid pixels decreased \u{2265}3 dB; 5% increased \u{2265}3 dB.",
                "OPERA DSWx-S1 (Sentinel-1) shows new water on 38% of changed area (threshold 20%).",
                "2.8 km of road lie within 250 m of changed pixels.",
            ]
        );
    }

    /// Celda r02c03 del fixture: sin observación OPERA.
    #[test]
    fn wording_matches_the_fixture_without_opera() {
        let c = cell(
            r#"{"cell_id":"piura-demo-r02c03","valid_fraction":0.93,"decrease_fraction":0.06,
                "increase_fraction":0.02,"changed_fraction":0.08,"p90_abs_delta_db":3.8,
                "opera_status":"unavailable","opera_overlap_ratio":null,
                "road_m_near_change":1200,"road_m_total":3500}"#,
        );
        assert_eq!(
            ranked(&config(), &c).unwrap()[1],
            "No OPERA DSWx-S1 (Sentinel-1) observation within 6 days; no independent contrast available."
        );
    }

    /// Celda r01c03 del fixture: la única que no pasa el gate.
    #[test]
    fn wording_matches_the_fixture_for_an_excluded_cell() {
        let c = cell(
            r#"{"cell_id":"piura-demo-r01c03","valid_fraction":0.55,"decrease_fraction":0.3,
                "increase_fraction":0.02,"changed_fraction":0.32,"p90_abs_delta_db":6.2,
                "opera_status":"not_corroborated","opera_overlap_ratio":0.1,
                "road_m_near_change":200,"road_m_total":800}"#,
        );
        assert_eq!(
            insufficient_quality(&config(), &c).unwrap(),
            vec!["Only 55% of pixels are valid (minimum 60%); excluded from the ranking."]
        );
    }

    #[test]
    fn the_threshold_comes_from_the_config() {
        let cfg = crate::RiskConfig::from_json(
            &crate::config::tests::config_json()
                .replace("\"delta_db_threshold\": 3.0", "\"delta_db_threshold\": 4.5"),
        )
        .unwrap();
        let c = cell(
            r#"{"cell_id":"piura-demo-r00c00","valid_fraction":0.95,"decrease_fraction":0.02,
                "increase_fraction":0.01,"changed_fraction":0.03,"p90_abs_delta_db":3.3,
                "opera_status":"not_corroborated","opera_overlap_ratio":0.05,
                "road_m_near_change":300,"road_m_total":2100}"#,
        );
        assert!(ranked(&cfg, &c).unwrap()[0].contains("\u{2265}4.5 dB"));
    }
}
