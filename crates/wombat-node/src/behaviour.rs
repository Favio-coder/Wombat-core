//! Behaviour del nodo libp2p.
//!
//! Este módulo solo se compila cuando se activa la característica `p2p`
//! (que incluye la dependencia `libp2p`). Cuando está desactivado, el nodo
//! corre en modo demo local sin red P2P.

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use wombat_core::{validate_wonb, Wonb};

/// Mensaje enviado sobre GossipSub: un WONB serializado en JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WonbMessage {
    pub wonb: Wonb,
}

#[cfg(feature = "p2p")]
use libp2p::{
    identify, ping,
    swarm::{NetworkBehaviour, SwarmEvent},
};

#[cfg(feature = "p2p")]
#[derive(NetworkBehaviour)]
pub struct WombatBehaviour {
    pub gossipsub: libp2p::gossipsub::Behaviour,
    pub identify: identify::Behaviour,
    pub ping: ping::Behaviour,
}

#[cfg(feature = "p2p")]
impl WombatBehaviour {
    pub fn new(
        gossipsub: libp2p::gossipsub::Behaviour,
        identify: identify::Behaviour,
        ping: ping::Behaviour,
    ) -> Self {
        Self {
            gossipsub,
            identify,
            ping,
        }
    }
}

/// Procesa un payload JSON recibido por GossipSub: lo deserializa a WONB y
/// lo valida. Disponible con o sin la característica `p2p`.
pub fn process_incoming_wonb(payload: &str) -> Result<Wonb, wombat_core::WonbError> {
    let wonb: Wonb =
        serde_json::from_str(payload).map_err(|e| wombat_core::WonbError::InvalidJson(e))?;
    validate_wonb(&wonb)?;
    info!(
        "WONB válido recibido: id={} tipo={} topic={}",
        wonb.id,
        wonb.type_,
        wonb.topic()
    );
    Ok(wonb)
}

#[cfg(feature = "p2p")]
/// Procesa un evento del swarm del nodo WOMBAT.
pub fn handle_swarm_event(
    event: libp2p::swarm::SwarmEvent<libp2p::swarm::SwarmEvent<WombatBehaviour>>,
) {
    // Punto de extensión documentado. El procesamiento real de mensajes se
    // hace en `process_incoming_wonb`.
    let _ = event;
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
                created_at: chrono::Utc::now().to_rfc3339(),
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
