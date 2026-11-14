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
}
