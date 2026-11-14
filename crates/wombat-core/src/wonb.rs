//! Modelo de datos WONB (Wombat Observation Notation Block).
//!
//! Un WONB es un bloque JSON de evidencia intercambiado entre nodos
//! heterogéneos del protocolo WOMBAT. Este módulo define los structs con
//! `serde` para serializar/deserializar el contrato mínimo especificado en
//! `docs/wonb-spec.md`.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Versión del esquema WONB implementada por este nodo.
pub const WONB_SPEC_VERSION: &str = "wombat/0.1";

/// Tipos de WONB soportados por el MVP.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WonbType {
    Anomaly,
    Context,
    Risk,
    Tool,
}

impl std::fmt::Display for WonbType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WonbType::Anomaly => write!(f, "anomaly"),
            WonbType::Context => write!(f, "context"),
            WonbType::Risk => write!(f, "risk"),
            WonbType::Tool => write!(f, "tool"),
        }
    }
}

/// Prioridad de enrutamiento de un WONB.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WonbPriority {
    Low,
    Medium,
    High,
}

/// Origen del WONB: quién lo generó y de qué fuente proviene.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WonbSource {
    pub provider: String,
    pub adapter: String,
    pub source_ref: String,
}

/// Proveniencia del WONB: cuándo, por quién, confianza, hash y regla.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WonbProvenance {
    pub created_at: String,
    pub created_by: String,
    pub confidence: f64,
    pub data_hash: String,
    pub rule_version: String,
}

/// Reglas de enrutamiento del WONB.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WonbRouting {
    pub topic: String,
    pub ttl_seconds: u64,
    pub priority: WonbPriority,
}

/// Estructura completa de un WONB.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wonb {
    pub spec_version: String,
    pub id: String,
    #[serde(rename = "type")]
    pub type_: WonbType,
    pub schema: String,
    pub source: WonbSource,
    pub payload: serde_json::Value,
    pub provenance: WonbProvenance,
    pub routing: WonbRouting,
    pub evidence_refs: Vec<String>,
    pub signature: String,
}

impl Wonb {
    /// Crea un nuevo WONB con los campos mínimos requeridos.
    ///
    /// El `id` se genera automáticamente como `wonb:{type}:{uuid}` si no se
    /// proporciona uno explícito.
    pub fn new(
        type_: WonbType,
        schema: impl Into<String>,
        source: WonbSource,
        payload: serde_json::Value,
        provenance: WonbProvenance,
        routing: WonbRouting,
        created_by: impl Into<String>,
    ) -> Self {
        let id = Uuid::new_v4();
        Self {
            spec_version: WONB_SPEC_VERSION.to_string(),
            id: format!("wonb:{}:{}", type_, id),
            type_,
            schema: schema.into(),
            source,
            payload,
            provenance: WonbProvenance {
                created_by: created_by.into(),
                ..provenance
            },
            routing,
            evidence_refs: Vec::new(),
            signature: String::new(),
        }
    }

    /// Devuelve el identificador del WONB.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Devuelve el topic de enrutamiento.
    pub fn topic(&self) -> &str {
        &self.routing.topic
    }

    /// Devuelve los segundos de TTL.
    pub fn ttl_seconds(&self) -> u64 {
        self.routing.ttl_seconds
    }

    /// Serializa el WONB a JSON canónico (orden de claves determinista,
    /// sin espacios innecesarios) para cálculo de hash.
    pub fn to_canonical_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }

    /// Serializa el WONB a JSON legible (pretty print).
    pub fn to_pretty_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }
}

impl Default for WonbPriority {
    fn default() -> Self {
        WonbPriority::Medium
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_provenance() -> WonbProvenance {
        WonbProvenance {
            created_at: "2026-01-01T00:00:00Z".to_string(),
            created_by: "did:wombat:node-demo".to_string(),
            confidence: 0.89,
            data_hash: "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                .to_string(),
            rule_version: "landslide-rule/0.1".to_string(),
        }
    }

    fn sample_routing() -> WonbRouting {
        WonbRouting {
            topic: "wombat/lima/costa-verde/anomaly".to_string(),
            ttl_seconds: 3600,
            priority: WonbPriority::High,
        }
    }

    fn sample_source() -> WonbSource {
        WonbSource {
            provider: "demo-geospatial-adapter".to_string(),
            adapter: "wombat-geospatial-python".to_string(),
            source_ref: "simulated-nisar-observation".to_string(),
        }
    }

    #[test]
    fn wonb_new_generates_deterministic_id_prefix() {
        let wonb = Wonb::new(
            WonbType::Anomaly,
            "geospatial.surface-displacement/1.0",
            sample_source(),
            serde_json::json!({}),
            sample_provenance(),
            sample_routing(),
            "did:wombat:node-demo",
        );
        assert!(wonb.id.starts_with("wonb:anomaly:"));
        assert_eq!(wonb.spec_version, WONB_SPEC_VERSION);
        assert_eq!(wonb.routing.topic, "wombat/lima/costa-verde/anomaly");
    }

    #[test]
    fn wonb_serialization_roundtrip() {
        let wonb = Wonb::new(
            WonbType::Risk,
            "risk.landslide/1.0",
            sample_source(),
            serde_json::json!({"score": 0.89}),
            sample_provenance(),
            sample_routing(),
            "did:wombat:node-demo",
        );
        let json = wonb.to_pretty_json().unwrap();
        let deserialized: Wonb = serde_json::from_str(&json).unwrap();
        assert_eq!(wonb, deserialized);
    }
}
