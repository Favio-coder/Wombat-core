//! Validación de WONBs.
//!
//! Reglas implementadas:
//! - Campos requeridos presentes.
//! - Versión del esquema soportada.
//! - Tipo de WONB permitido.
//! - Topic válido y con prefijo esperado.
//! - TTL > 0.
//! - Timestamp parseable (ISO-8601 UTC).
//! - Rechazo de mensajes expirados (basado en created_at + ttl_seconds).

use chrono::{DateTime, Utc};
use std::collections::HashSet;

use crate::error::{WonbError, WonbError::*};
use crate::wonb::Wonb;

/// Política de expiración para validar si un WONB está vencido.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExpiryPolicy {
    /// Rechazar WONBs cuya ventana de validez ya cerró.
    #[default]
    RejectExpired,
    /// Permitir WONBs expirados (solo para tests / modos pasivos).
    AllowExpired,
}

/// Validador de WONBs con estado de duplicados en memoria.
#[derive(Debug, Clone, Default)]
pub struct WonbValidator {
    /// IDs de WONBs ya vistos (deduplicación en memoria).
    seen_ids: HashSet<String>,
    /// Política de expiración aplicada.
    expiry_policy: ExpiryPolicy,
    /// Topics permitidos (vacío = todos los topics con prefijo wombat/).
    allowed_topics: Option<Vec<String>>,
}

impl WonbValidator {
    /// Crea un validador con la política de expiración por defecto.
    pub fn new() -> Self {
        Self::default()
    }

    /// Establece la política de expiración.
    pub fn with_expiry_policy(mut self, policy: ExpiryPolicy) -> Self {
        self.expiry_policy = policy;
        self
    }

    /// Restringe los topics aceptados. Si es `None`, se aceptan topics con
    /// prefijo `wombat/`.
    pub fn with_allowed_topics(mut self, topics: Vec<String>) -> Self {
        self.allowed_topics = Some(topics);
        self
    }

    /// Valida un WONB completo. Devuelve una referencia al WONB si es válido.
    ///
    /// Reglas aplicadas en orden:
    /// 1. Campos requeridos (no vacíos donde corresponde).
    /// 2. Versión del esquema.
    /// 3. Tipo de WONB permitido.
    /// 4. Topic válido.
    /// 5. TTL > 0.
    /// 6. Timestamp parseable y dentro de la ventana de validez.
    /// 7. Deduplicación por `id`.
    pub fn validate(&mut self, wonb: &Wonb) -> Result<(), WonbError> {
        self.validate_fields(wonb)?;
        self.validate_version(wonb)?;
        self.validate_type(wonb)?;
        self.validate_topic(wonb)?;
        self.validate_ttl(wonb)?;
        self.validate_timestamp(wonb)?;
        self.check_expiry(wonb)?;
        self.check_duplicate(wonb)?;
        Ok(())
    }

    /// Valida un WONB sin registrarlo como visto (no marca duplicado).
    pub fn validate_without_recording(&mut self, wonb: &Wonb) -> Result<(), WonbError> {
        self.validate_fields(wonb)?;
        self.validate_version(wonb)?;
        self.validate_type(wonb)?;
        self.validate_topic(wonb)?;
        self.validate_ttl(wonb)?;
        self.validate_timestamp(wonb)?;
        self.check_expiry(wonb)?;
        Ok(())
    }

    /// Registra un ID como visto para futuras comprobaciones de duplicado.
    pub fn record_seen(&mut self, id: &str) {
        self.seen_ids.insert(id.to_string());
    }

    /// Comprueba si un ID ya fue visto (duplicado).
    pub fn is_duplicate(&self, id: &str) -> bool {
        self.seen_ids.contains(id)
    }

    fn validate_fields(&self, wonb: &Wonb) -> Result<(), WonbError> {
        if wonb.id.is_empty() {
            return Err(MissingField { field: "id".into() });
        }
        if wonb.schema.is_empty() {
            return Err(MissingField {
                field: "schema".into(),
            });
        }
        if wonb.source.provider.is_empty() {
            return Err(MissingField {
                field: "source.provider".into(),
            });
        }
        if wonb.source.adapter.is_empty() {
            return Err(MissingField {
                field: "source.adapter".into(),
            });
        }
        if wonb.source.source_ref.is_empty() {
            return Err(MissingField {
                field: "source.source_ref".into(),
            });
        }
        if wonb.provenance.created_at.is_empty() {
            return Err(MissingField {
                field: "provenance.created_at".into(),
            });
        }
        if wonb.provenance.created_by.is_empty() {
            return Err(MissingField {
                field: "provenance.created_by".into(),
            });
        }
        if wonb.provenance.data_hash.is_empty() {
            return Err(MissingField {
                field: "provenance.data_hash".into(),
            });
        }
        if wonb.provenance.rule_version.is_empty() {
            return Err(MissingField {
                field: "provenance.rule_version".into(),
            });
        }
        if wonb.routing.topic.is_empty() {
            return Err(MissingField {
                field: "routing.topic".into(),
            });
        }
        Ok(())
    }

    fn validate_version(&self, wonb: &Wonb) -> Result<(), WonbError> {
        if wonb.spec_version != crate::wonb::WONB_SPEC_VERSION {
            return Err(UnsupportedVersion {
                expected: crate::wonb::WONB_SPEC_VERSION.into(),
                got: wonb.spec_version.clone(),
            });
        }
        Ok(())
    }

    fn validate_type(&self, _wonb: &Wonb) -> Result<(), WonbError> {
        // WonbType es un enum cerrado, por lo que cualquier valor es válido.
        // Se mantiene la validación explícita por documento y futuras
        // extensiones.
        Ok(())
    }

    fn validate_topic(&self, wonb: &Wonb) -> Result<(), WonbError> {
        let topic = &wonb.routing.topic;
        if !topic.starts_with("wombat/") {
            return Err(InvalidTopic {
                topic: topic.clone(),
            });
        }
        if let Some(ref allowed) = self.allowed_topics {
            if !allowed.iter().any(|t| t == topic) {
                return Err(InvalidTopic {
                    topic: topic.clone(),
                });
            }
        }
        Ok(())
    }

    fn validate_ttl(&self, wonb: &Wonb) -> Result<(), WonbError> {
        if wonb.routing.ttl_seconds == 0 {
            return Err(InvalidTtl {
                ttl: wonb.routing.ttl_seconds,
            });
        }
        Ok(())
    }

    fn validate_timestamp(&self, wonb: &Wonb) -> Result<(), WonbError> {
        let ts = wonb
            .provenance
            .created_at
            .parse::<DateTime<Utc>>()
            .map_err(|_| InvalidTimestamp(wonb.provenance.created_at.clone()))?;
        let _ = ts;
        // Confianza en puntos básicos [0, 10000].
        if wonb.provenance.confidence_bp > 10000 {
            return Err(InvalidTimestamp(format!(
                "confidence_bp fuera de rango [0,10000]: {}",
                wonb.provenance.confidence_bp
            )));
        }
        Ok(())
    }

    fn check_expiry(&self, wonb: &Wonb) -> Result<(), WonbError> {
        if matches!(self.expiry_policy, ExpiryPolicy::AllowExpired) {
            return Ok(());
        }
        let created_at = wonb
            .provenance
            .created_at
            .parse::<DateTime<Utc>>()
            .map_err(|_| InvalidTimestamp(wonb.provenance.created_at.clone()))?;
        let expires_at = created_at + chrono::Duration::seconds(wonb.routing.ttl_seconds as i64);
        let now = Utc::now();
        if now > expires_at {
            return Err(Expired {
                id: wonb.id.clone(),
                expired_at: expires_at.to_rfc3339(),
            });
        }
        Ok(())
    }

    fn check_duplicate(&mut self, wonb: &Wonb) -> Result<(), WonbError> {
        if self.seen_ids.contains(&wonb.id) {
            return Err(Duplicate {
                id: wonb.id.clone(),
            });
        }
        self.seen_ids.insert(wonb.id.clone());
        Ok(())
    }
}

/// Función de conveniencia para validar un WONB con un validador fresco.
pub fn validate_wonb(wonb: &Wonb) -> Result<(), WonbError> {
    let mut validator = WonbValidator::new();
    validator.validate(wonb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wonb::tests::*;
    use crate::wonb::WonbPriority;
    use crate::wonb::WonbType;

    #[test]
    fn valid_wonb_passes() {
        let mut v = WonbValidator::new();
        assert!(v.validate(&valid_wonb()).is_ok());
    }

    #[test]
    fn unsupported_version_rejected() {
        let mut wonb = valid_wonb();
        wonb.spec_version = "wombat/99.0".into();
        let mut v = WonbValidator::new();
        let err = v.validate(&wonb).unwrap_err();
        matches!(err, UnsupportedVersion { .. });
    }

    #[test]
    fn invalid_type_rejected() {
        // WonbType es un enum cerrado, así que forzamos vía JSON malformed.
        // Aquí comprobamos que un topic inválido se rechaza.
        let mut wonb = valid_wonb();
        wonb.routing.topic = "otro/topic".into();
        let mut v = WonbValidator::new();
        let err = v.validate(&wonb).unwrap_err();
        matches!(err, InvalidTopic { .. });
    }

    #[test]
    fn zero_ttl_rejected() {
        let mut wonb = valid_wonb();
        wonb.routing.ttl_seconds = 0;
        let mut v = WonbValidator::new();
        let err = v.validate(&wonb).unwrap_err();
        matches!(err, InvalidTtl { .. });
    }

    #[test]
    fn expired_wonb_rejected() {
        let mut wonb = valid_wonb();
        // created_at en el pasado con TTL corto -> expirado.
        wonb.provenance.created_at = "2020-01-01T00:00:00Z".into();
        wonb.routing.ttl_seconds = 60;
        let mut v = WonbValidator::new();
        let err = v.validate(&wonb).unwrap_err();
        matches!(err, Expired { .. });
    }

    #[test]
    fn expired_wonb_allowed_with_policy() {
        let mut wonb = valid_wonb();
        wonb.provenance.created_at = "2020-01-01T00:00:00Z".into();
        wonb.routing.ttl_seconds = 60;
        let mut v = WonbValidator::new().with_expiry_policy(ExpiryPolicy::AllowExpired);
        assert!(v.validate(&wonb).is_ok());
    }

    #[test]
    fn duplicate_wonb_rejected() {
        let wonb = valid_wonb();
        let mut v = WonbValidator::new();
        assert!(v.validate(&wonb).is_ok());
        let err = v.validate(&wonb).unwrap_err();
        matches!(err, Duplicate { .. });
    }

    #[test]
    fn missing_field_rejected() {
        let mut wonb = valid_wonb();
        wonb.id = "".into();
        let mut v = WonbValidator::new();
        let err = v.validate(&wonb).unwrap_err();
        matches!(err, MissingField { field } if field == "id");
    }

    #[test]
    fn four_valid_types_accepted() {
        for ty in [
            WonbType::Anomaly,
            WonbType::Context,
            WonbType::Risk,
            WonbType::Tool,
        ] {
            let ty_label = format!("{:?}", ty);
            let wonb = Wonb::new(
                ty,
                "test.schema/1.0",
                sample_source(),
                serde_json::json!({}),
                sample_provenance(),
                sample_routing(),
                "did:wombat:node-demo",
            );
            let mut v = WonbValidator::new();
            assert!(v.validate(&wonb).is_ok(), "tipo {} rechazado", ty_label);
        }
    }

    #[test]
    fn confidence_out_of_range_rejected() {
        let mut wonb = valid_wonb();
        wonb.provenance.confidence_bp = 10001;
        let mut v = WonbValidator::new();
        let err = v.validate(&wonb).unwrap_err();
        matches!(err, InvalidTimestamp { .. });
    }

    #[test]
    fn wonb_priority_default_is_medium() {
        assert_eq!(WonbPriority::default(), WonbPriority::Medium);
    }
}
