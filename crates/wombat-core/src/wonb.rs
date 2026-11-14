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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WonbPriority {
    Low,
    #[default]
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
///
/// `confidence_bp` está en puntos básicos (0–10000, donde 10000 = 100.00%).
/// Esto sigue la convención R16 del proyecto: sin `f64` en contenido firmado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WonbProvenance {
    pub created_at: String,
    pub created_by: String,
    /// Confianza del emisor en el bloque, en puntos básicos (0–10000).
    pub confidence_bp: u16,
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

    /// Serializa el WONB a JSON canónico JCS (RFC 8785: claves ordenadas,
    /// sin espacios innecesarios) para cálculo de hash y firma.
    ///
    /// Usa `serde_jcs` en lugar de `serde_json::to_string`, igual que WONB 0.2,
    /// para garantizar bytes deterministas entre Rust, Python y JavaScript.
    pub fn to_canonical_json(&self) -> serde_json::Result<String> {
        let bytes = serde_jcs::to_vec(self)?;
        String::from_utf8(bytes)
            .map_err(|e| serde::ser::Error::custom(format!("JCS produjo UTF-8 inválido: {e}")))
    }

    /// Serializa el WONB a JSON legible (pretty print).
    pub fn to_pretty_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn sample_provenance() -> WonbProvenance {
        WonbProvenance {
            created_at: chrono::Utc::now().to_rfc3339(),
            created_by: "did:wombat:node-demo".to_string(),
            confidence_bp: 8900,
            data_hash: "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                .to_string(),
            rule_version: "landslide-rule/0.1".to_string(),
        }
    }

    pub(crate) fn sample_routing() -> WonbRouting {
        WonbRouting {
            topic: "wombat/lima/costa-verde/anomaly".to_string(),
            ttl_seconds: 3600,
            priority: WonbPriority::High,
        }
    }

    pub(crate) fn sample_source() -> WonbSource {
        WonbSource {
            provider: "demo-geospatial-adapter".to_string(),
            adapter: "wombat-geospatial-python".to_string(),
            source_ref: "simulated-nisar-observation".to_string(),
        }
    }

    pub(crate) fn valid_wonb() -> Wonb {
        Wonb::new(
            WonbType::Anomaly,
            "geospatial.surface-displacement/1.0",
            sample_source(),
            serde_json::json!({"displacement_cm": 3.4, "days": 12, "slope_deg": 34.2}),
            sample_provenance(),
            sample_routing(),
            "did:wombat:node-demo",
        )
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
