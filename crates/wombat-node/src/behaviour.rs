//! Behaviour del nodo libp2p.
//!
//! Combina GossipSub (publicación/suscripción de WONBs), Identify y Ping.
//! Los mensajes recibidos se validan con `wombat-core` antes de ser
//! procesados o re-publicados.

use libp2p::{
    identify, ping,
    swarm::{NetworkBehaviour, SwarmEvent},
};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};
use wombat_core::{validate_wonb, Wonb};

/// Mensaje enviado sobre GossipSub: un WONB serializado en JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WonbMessage {
    pub wonb: Wonb,
}

#[derive(NetworkBehaviour)]
pub struct WombatBehaviour {
    pub gossipsub: libp2p::gossipsub::Behaviour,
    pub identify: identify::Behaviour,
    pub ping: ping::Behaviour,
}

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

impl SwarmEvent<WombatBehaviour> {
    // Placeholder para manejar eventos del swarm de forma tipada.
}

pub fn handle_swarm_event(
    event: libp2p::swarm::SwarmEvent<WombatBehaviourEvent>,
) {
    match event {
        SwarmEvent::NewListenAddr { address, .. } => {
            info!("Nodo escuchando en {}", address);
        }
        SwarmEvent::Behaviour(WombatBehaviourEvent::GossipSub(
            libp2p::gossipsub::Event::Message {
                propagation_source: _,
                message_id: _,
                message,
            },
        )) => {
            let payload = String::from_utf8_lossy(&message.data);
            debug!("WONB recibido en topic {}: {}", message.topic, payload);
            if let Err(e) = process_incoming_wonb(&payload) {
                warn!("WONB recibido inválido: {}", e);
            }
        }
        SwarmEvent::Behaviour(WombatBehaviourEvent::Identify(
            identify::Event::Received { info, .. },
        )) => {
            info!("Identify recibido de peer: {:?}", info.peer_id);
        }
        SwarmEvent::Behaviour(WombatBehaviourEvent::Ping(ping::Event)) => {
            debug!("Evento de ping recibido");
        }
        _ => {}
    }
}

fn process_incoming_wonb(payload: &str) -> Result<(), wombat_core::WonbError> {
    let wonb: Wonb = serde_json::from_str(payload)
        .map_err(|e| wombat_core::WonbError::InvalidJson(e))?;
    validate_wonb(&wonb)?;
    info!("WONB válido recibido: id={} tipo={} topic={}", wonb.id, wonb.type_, wonb.topic());
    Ok(())
}

/// Eventos unificados del behaviour (re-exportados para el swarm).
pub enum WombatBehaviourEvent {
    GossipSub(libp2p::gossipsub::Event),
    Identify(identify::Event),
    Ping(ping::Event),
}

impl From<libp2p::gossipsub::Event> for WombatBehaviourEvent {
    fn from(e: libp2p::gossipsub::Event) -> Self {
        WombatBehaviourEvent::GossipSub(e)
    }
}

impl From<identify::Event> for WombatBehaviourEvent {
    fn from(e: identify::Event) -> Self {
        WombatBehaviourEvent::Identify(e)
    }
}

impl From<ping::Event> for WombatBehaviourEvent {
    fn from(e: ping::Event) -> Self {
        WombatBehaviourEvent::Ping(e)
    }
}
