//! `risk-config.json`: la configuración manda sobre cualquier umbral escrito en
//! el código.
//!
//! No hay un solo número de la regla de riesgo en el código. Los pesos, los
//! umbrales de estado, la saturación y el gate de calidad vienen de aquí, y el
//! test `changing_a_weight_changes_the_ranking` existe precisamente para probar
//! que nada se colonizó dentro de `evaluate`.
//!
//! El JSON trae decimales (`0.45`). Al cargar se convierten **una sola vez** a
//! enteros en puntos básicos, y de ahí en adelante no vuelve a aparecer un
//! `f64` en la ruta que produce el payload (R16).

use serde::Deserialize;

use crate::error::RiskError;
use crate::exact::Exact;

/// Configuración ya normalizada a enteros, lista para calcular.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiskConfig {
    /// `risk-config/0.1`. Va al `method` de la proveniencia de cada bloque.
    pub version: String,
    /// Umbral de cambio en dB, exacto, para el texto y para los centi-dB.
    pub delta_db_threshold: Exact,
    /// Gate de calidad: por debajo de esto la celda no entra al ranking.
    pub quality_min_valid_fraction_bp: u32,
    /// Fracción de cambio que ya satura el componente `change`.
    pub change_saturation: Exact,
    pub opera_max_gap_days: u32,
    pub corroboration_overlap_threshold_bp: u32,
    pub road_buffer_meters: u32,
    pub weights_bp: WeightsBp,
    pub status_thresholds_bp: StatusThresholdsBp,
    pub top_n_actions: usize,
    pub ttl_seconds: TtlSeconds,
}

/// Pesos de los tres componentes, en puntos básicos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeightsBp {
    pub change: u32,
    pub opera: u32,
    pub road: u32,
}

/// Umbrales de estado sobre `score_bp`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusThresholdsBp {
    pub review_high: u32,
    pub review_medium: u32,
}

/// Vida útil de cada tipo de bloque, en segundos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TtlSeconds {
    pub risk: u64,
    pub action: u64,
}

impl RiskConfig {
    /// Lee la configuración desde el JSON del contrato.
    pub fn from_json(s: &str) -> Result<Self, RiskError> {
        RawConfig::deserialize(&mut serde_json::Deserializer::from_str(s))?.normalize()
    }

    /// Umbral de cambio en centi-dB, para el payload del bloque `change`.
    pub fn delta_db_threshold_cdb(&self) -> Result<u32, RiskError> {
        let scaled = self.delta_db_threshold.mul(Exact::int(100))?;
        let text = scaled.round_half_even_str(0)?;
        text.parse::<u32>()
            .map_err(|e| RiskError::Config(format!("delta_db_threshold en cdB: {e}")))
    }
}

/// La configuración tal como está en el JSON, con sus decimales.
///
/// `deny_unknown_fields` replica el `additionalProperties: false` del esquema:
/// una clave nueva en el contrato no debe pasar inadvertida.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    version: String,
    delta_db_threshold: f64,
    quality_min_valid_fraction: f64,
    change_saturation: f64,
    opera_max_gap_days: u32,
    corroboration_overlap_threshold: f64,
    road_buffer_meters: u32,
    weights: RawWeights,
    status_thresholds: RawStatusThresholds,
    top_n_actions: usize,
    ttl_seconds: RawTtl,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWeights {
    change: f64,
    opera: f64,
    road: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStatusThresholds {
    review_high: f64,
    review_medium: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTtl {
    risk: u64,
    action: u64,
}

impl RawConfig {
    fn normalize(self) -> Result<RiskConfig, RiskError> {
        let saturation = Exact::from_f64_repr(self.change_saturation)?;
        if saturation.cmp_exact(&Exact::ZERO) != std::cmp::Ordering::Greater {
            return Err(RiskError::Config(
                "change_saturation debe ser > 0: es un divisor".into(),
            ));
        }
        if self.top_n_actions == 0 {
            return Err(RiskError::Config("top_n_actions debe ser >= 1".into()));
        }
        Ok(RiskConfig {
            version: self.version,
            delta_db_threshold: Exact::from_f64_repr(self.delta_db_threshold)?,
            quality_min_valid_fraction_bp: bp(self.quality_min_valid_fraction)?,
            change_saturation: saturation,
            opera_max_gap_days: self.opera_max_gap_days,
            corroboration_overlap_threshold_bp: bp(self.corroboration_overlap_threshold)?,
            road_buffer_meters: self.road_buffer_meters,
            weights_bp: WeightsBp {
                change: bp(self.weights.change)?,
                opera: bp(self.weights.opera)?,
                road: bp(self.weights.road)?,
            },
            status_thresholds_bp: StatusThresholdsBp {
                review_high: bp(self.status_thresholds.review_high)?,
                review_medium: bp(self.status_thresholds.review_medium)?,
            },
            top_n_actions: self.top_n_actions,
            ttl_seconds: TtlSeconds {
                risk: self.ttl_seconds.risk,
                action: self.ttl_seconds.action,
            },
        })
    }
}

fn bp(x: f64) -> Result<u32, RiskError> {
    Exact::from_f64_repr(x)?.to_bp_u32()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// La config del contrato, tal como está en `config/risk-config.json`.
    pub(crate) fn config_json() -> &'static str {
        r#"{
          "version": "risk-config/0.1",
          "delta_db_threshold": 3.0,
          "quality_min_valid_fraction": 0.60,
          "change_saturation": 0.25,
          "opera_max_gap_days": 6,
          "corroboration_overlap_threshold": 0.20,
          "road_buffer_meters": 250,
          "weights": { "change": 0.45, "opera": 0.35, "road": 0.20 },
          "status_thresholds": { "review_high": 0.60, "review_medium": 0.35 },
          "top_n_actions": 3,
          "ttl_seconds": { "risk": 86400, "action": 3600 }
        }"#
    }

    pub(crate) fn config() -> RiskConfig {
        RiskConfig::from_json(config_json()).unwrap()
    }

    #[test]
    fn decimals_become_basis_points() {
        let cfg = config();
        assert_eq!(cfg.weights_bp.change, 4500);
        assert_eq!(cfg.weights_bp.opera, 3500);
        assert_eq!(cfg.weights_bp.road, 2000);
        assert_eq!(cfg.status_thresholds_bp.review_high, 6000);
        assert_eq!(cfg.status_thresholds_bp.review_medium, 3500);
        assert_eq!(cfg.quality_min_valid_fraction_bp, 6000);
        assert_eq!(cfg.corroboration_overlap_threshold_bp, 2000);
    }

    #[test]
    fn delta_threshold_keeps_its_exact_value() {
        let cfg = config();
        assert_eq!(cfg.delta_db_threshold_cdb().unwrap(), 300);
        assert_eq!(cfg.delta_db_threshold.trimmed_decimal().unwrap(), "3");
    }

    #[test]
    fn unknown_config_field_rejected() {
        let json = config_json().replace("\"top_n_actions\": 3", "\"top_n_acciones\": 3");
        assert!(RiskConfig::from_json(&json).is_err());
    }

    #[test]
    fn zero_saturation_rejected_because_it_is_a_divisor() {
        let json = config_json().replace("\"change_saturation\": 0.25", "\"change_saturation\": 0");
        let err = RiskConfig::from_json(&json).unwrap_err();
        assert!(matches!(err, RiskError::Config(_)), "{err}");
    }

    #[test]
    fn zero_top_n_rejected() {
        let json = config_json().replace("\"top_n_actions\": 3", "\"top_n_actions\": 0");
        let err = RiskConfig::from_json(&json).unwrap_err();
        assert!(matches!(err, RiskError::Config(_)), "{err}");
    }
}
