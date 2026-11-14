//! Behaviour del nodo WOMBAT.
//!
//! Procesa mensajes recibidos (por ejemplo, vía GossipSub cuando libp2p esté
//! habilitado) deserializándolos a WONB y validándolos con `wombat-core`.
//!
//! Nota: el stack libp2p no es una dependencia directa en esta versión del
//! workspace debido a que el índice de crates.io disponible no resuelve su
//! cadena de dependencias (core2 0.4.0 yanked). El código específico de
//! libp2p queda documentado y listo para activarse cuando la dependencia esté
//! disponible.

use serde::{Deserialize, Serialize};
use tracing::info;
use wombat_core::{validate_wonb, Wonb};

/// Mensaje enviado sobre GossipSub: un WONB serializado en JSON.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WonbMessage {
    pub wonb: Wonb,
}

/// Procesa un payload JSON recibido: lo deserializa a WONB y lo valida.
#[allow(dead_code)]
pub fn process_incoming_wonb(payload: &str) -> Result<Wonb, wombat_core::WonbError> {
    let wonb: Wonb = serde_json::from_str(payload).map_err(wombat_core::WonbError::InvalidJson)?;
    validate_wonb(&wonb)?;
    info!(
        "WONB válido recibido: id={} tipo={} topic={}",
        wonb.id,
        wonb.type_,
        wonb.topic()
    );
    Ok(wonb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DEFAULT_TOPICS;

    #[test]
    fn default_topics_have_wombat_prefix() {
        for t in DEFAULT_TOPICS {
            assert!(t.starts_with("wombat/"));
        }
    }

    #[test]
    fn process_incoming_wonb_valid_accepted() {
        let wonb = wombat_core::wonb::Wonb::new(
            wombat_core::wonb::WonbType::Anomaly,
            "geospatial.surface-displacement/1.0",
            wombat_core::wonb::WonbSource {
                provider: "demo".into(),
                adapter: "wombat-geospatial-python".into(),
                source_ref: "simulated".into(),
            },
            serde_json::json!({"displacement_cm": 3.4}),
            wombat_core::wonb::WonbProvenance {
                created_at: "2027-01-15T12:00:00Z".to_string(),
                created_by: "did:wombat:node-demo".into(),
                confidence: 0.89,
                data_hash:
                    "sha256:0000000000000000000000000000000000000000000000000000000000000000".into(),
                rule_version: "landslide-rule/0.1".into(),
            },
            wombat_core::wonb::WonbRouting {
                topic: "wombat/lima/costa-verde/anomaly".into(),
                ttl_seconds: 3600,
                priority: wombat_core::wonb::WonbPriority::High,
            },
            "did:wombat:node-demo",
        );
        let json = serde_json::to_string(&wonb).unwrap();
        let result = process_incoming_wonb(&json);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().id, wonb.id);
    }

    #[test]
    fn process_incoming_wonb_invalid_json_rejected() {
        let err = process_incoming_wonb("no es json").unwrap_err();
        matches!(err, wombat_core::WonbError::InvalidJson { .. });
    }
}
