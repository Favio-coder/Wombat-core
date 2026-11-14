// Código comentado de Favio

// pub fn add(left: u64, right: u64) -> u64 {
//     left + right
// }

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn it_works() {
//         let result = add(2, 2);
//         assert_eq!(result, 4);
//     }
// }

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellInput {
    pub cell_id: String,
    pub changed_fraction: f64,
    pub valid_fraction: f64,
    pub opera_overlap_ratio: Option<f64>,
    pub road_m_near_change: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskStatus {
    InsufficientQuality,
    ReviewHigh,
    ReviewMedium,
    ReviewLow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskConfig {
    pub quality_min_valid_fraction: f64,
    pub change_saturation: f64,
    pub weight_change: f64,
    pub weight_opera: f64,
    pub weight_road: f64,
    pub review_high_threshold: f64,
    pub review_medium_threshold: f64,
}

#[derive(Debug, Serialize, Clone, Deserialize)]
pub struct RiskResult {
    pub cell_id: String,
    pub score: f64,
    pub status: RiskStatus,
}

pub fn passes_quality_gate(cell: &CellInput, config: &RiskConfig) -> bool {
    cell.valid_fraction >= config.quality_min_valid_fraction
}

//Componente de cambio
pub fn change_component(cell: &CellInput, config: &RiskConfig) -> f64 {
    if config.change_saturation <= 0.0 {
        return 0.0;
    }

    (cell.changed_fraction / config.change_saturation).clamp(0.0, 1.0)
}

// Componente OPERA
pub fn opera_component(cell: &CellInput) -> f64 {
    cell.opera_overlap_ratio.unwrap_or(0.0).clamp(0.0, 1.0)
}

pub fn road_component(cell: &CellInput, cells: &[CellInput]) -> f64 {
    let max_road = cells
        .iter()
        .map(|candidate| candidate.road_m_near_change)
        .fold(0.0_f64, f64::max);

    if max_road <= 0.0 {
        return 0.0;
    }

    (cell.road_m_near_change / max_road).clamp(0.0, 1.0)
}

pub fn calculate_score(cell: &CellInput, cells: &[CellInput], config: &RiskConfig) -> Option<f64> {
    if !passes_quality_gate(cell, config) {
        return None;
    }

    let score = config.weight_change * change_component(cell, config)
        + config.weight_opera * opera_component(cell)
        + config.weight_road * road_component(cell, cells);

    Some(score.clamp(0.0, 1.0))
}

// Validar Score
pub fn classify_score(score: f64, config: &RiskConfig) -> RiskStatus {
    if score >= config.review_high_threshold {
        RiskStatus::ReviewHigh
    } else if score >= config.review_medium_threshold {
        RiskStatus::ReviewMedium
    } else {
        RiskStatus::ReviewLow
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    fn config() -> RiskConfig {
        RiskConfig {
            quality_min_valid_fraction: 0.6,
            change_saturation: 0.4,
            weight_change: 0.5,
            weight_opera: 0.3,
            weight_road: 0.2,
            review_high_threshold: 0.6,
            review_medium_threshold: 0.35,
        }
    }

    fn cell(valid_fraction: f64) -> CellInput {
        CellInput {
            cell_id: "piura-001".to_string(),
            changed_fraction: 0.2,
            valid_fraction,
            opera_overlap_ratio: None,
            road_m_near_change: 100.0,
        }
    }

    #[test]
    fn excludes_cell_below_minimum_quality() {
        assert!(!passes_quality_gate(&cell(0.59), &config()));
    }

    #[test]
    fn accepts_cell_at_minimum_quality() {
        assert!(passes_quality_gate(&cell(0.60), &config()));
    }

    //Test
    #[test]
    fn calculates_change_component() {
        let result = change_component(&cell(0.80), &config());

        assert!((result - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn caps_change_component_at_one() {
        let mut high_change_cell = cell(0.80);
        high_change_cell.changed_fraction = 0.90;

        let result = change_component(&high_change_cell, &config());
        assert!((result - 1.0).abs() < f64::EPSILON);
    }

    // Test módulo Opera
    #[test]
    fn uses_opera_overlap_when_available() {
        let mut corroborated_cell = cell(0.80);
        corroborated_cell.opera_overlap_ratio = Some(0.70);

        let result = opera_component(&corroborated_cell);
        assert!((result - 0.70).abs() < f64::EPSILON);
    }

    #[test]
    fn uses_zero_when_opera_is_unavailable() {
        let result = opera_component(&cell(0.80));
        assert!((result - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn normalizes_road_exposure_against_the_highest_cell() {
        let mut first = cell(0.80);
        first.road_m_near_change = 100.0;

        let mut second = cell(0.80);
        second.cell_id = "piura-002".to_string();
        second.road_m_near_change = 200.0;

        let cells = vec![first.clone(), second.clone()];

        assert!((road_component(&first, &cells) - 0.5).abs() < f64::EPSILON);
        assert!((road_component(&second, &cells) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn returns_zero_when_no_roads_are_available() {
        let mut no_road_cell = cell(0.80);
        no_road_cell.road_m_near_change = 0.0;

        let cells = vec![no_road_cell.clone()];

        assert!((road_component(&no_road_cell, &cells) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn calculates_weighted_score_for_valid_cell() {
        let mut first = cell(0.80);
        first.opera_overlap_ratio = Some(0.70);
        first.road_m_near_change = 100.0;

        let mut second = cell(0.80);
        second.cell_id = "piura-002".to_string();
        second.road_m_near_change = 200.0;

        let cells = vec![first.clone(), second];

        let score = calculate_score(&first, &cells, &config());

        let score = score.expect("La celda valida debe tener Score");
        assert!((score - 0.56).abs() < 1e-9);
    }

    #[test]
    fn does_not_score_cell_with_insufficient_quality() {
        let low_quality_cell = cell(0.59);
        let cells = vec![low_quality_cell.clone()];

        assert_eq!(calculate_score(&low_quality_cell, &cells, &config()), None);
    }

    #[test]
    fn classifies_high_priority_score() {
        assert_eq!(classify_score(0.60, &config()), RiskStatus::ReviewHigh);
    }

    #[test]
    fn classifies_medium_priority_score() {
        assert_eq!(classify_score(0.35, &config()), RiskStatus::ReviewMedium);
    }

    #[test]
    fn classifies_low_priority_score() {
        assert_eq!(classify_score(0.34, &config()), RiskStatus::ReviewLow);
    }
}
