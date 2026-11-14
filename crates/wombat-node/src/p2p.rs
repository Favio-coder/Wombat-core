use std::{collections::HashSet, path::Path, time::Duration};

use anyhow::{bail, Context, Result};
use chrono::{SecondsFormat, Utc};
use futures_util::StreamExt;
use libp2p::{
    gossipsub::{self, IdentTopic, MessageAcceptance, TopicHash},
    multiaddr::Protocol,
    noise,
    swarm::SwarmEvent,
    tcp, yamux, Multiaddr, PeerId, Swarm, SwarmBuilder,
};
use tracing::{error, info, warn};
use wombat_core::v02::{verify_chain, Block, TrustedIssuers};

use crate::config::NodeConfig;

type NodeSwarm = Swarm<gossipsub::Behaviour>;

struct PendingPublication {
    topic: IdentTopic,
    topic_hash: TopicHash,
    block_id: String,
    bytes: Vec<u8>,
}

pub async fn run(config: NodeConfig) -> Result<()> {
    let trusted_path = config
        .trusted_issuers
        .as_deref()
        .context("--trusted-issuers es obligatorio para el modo P2P")?;
    let trusted = load_trusted_issuers(trusted_path)?;

    let mut publication = config
        .publish
        .as_deref()
        .map(|path| load_publication(path, &trusted))
        .transpose()?;

    let mut topics = config.topics;
    if let Some(pending) = &publication {
        if !topics
            .iter()
            .any(|topic| topic == pending.topic_hash.as_str())
        {
            topics.push(pending.topic_hash.as_str().to_owned());
        }
    }
    if topics.is_empty() {
        bail!("indica al menos un --topic o un --publish con un WONB válido");
    }

    let mut swarm = new_swarm()?;
    let local_peer_id = *swarm.local_peer_id();
    let listen_addr: Multiaddr = config
        .listen_addr
        .parse()
        .with_context(|| format!("multiaddr --listen inválida: {}", config.listen_addr))?;
    swarm.listen_on(listen_addr)?;

    for topic in &topics {
        swarm
            .behaviour_mut()
            .subscribe(&IdentTopic::new(topic.clone()))
            .with_context(|| format!("no se pudo suscribir al topic {topic}"))?;
        info!(topic, "Suscrito localmente a GossipSub");
    }

    let mut expected_peers = HashSet::new();
    for address in &config.peers {
        let (address, peer_id) = parse_peer_address(address)?;
        swarm.dial(address.clone())?;
        info!(%peer_id, %address, "Marcado dial explícito al peer");
        expected_peers.insert(peer_id);
    }

    let timeout = Duration::from_secs(config.timeout_secs);
    let deadline = tokio::time::Instant::now() + timeout;
    let mut connected_peers = HashSet::new();

    info!(%local_peer_id, "PeerId local");
    info!(
        listen = %config.listen_addr,
        "Iniciando escucha TCP"
    );

    while !expected_peers.is_subset(&connected_peers) || publication.is_some() {
        let event = tokio::select! {
            _ = tokio::time::sleep_until(deadline) => {
                bail!("timeout de {}s esperando conexión o suscripción remota", timeout.as_secs());
            }
            signal = tokio::signal::ctrl_c() => {
                signal.context("no se pudo esperar Ctrl-C")?;
                info!("Interrumpido por el usuario");
                return Ok(());
            }
            event = swarm.select_next_some() => event,
        };

        match event {
            SwarmEvent::NewListenAddr { address, .. } => {
                let dialable = address.with(Protocol::P2p(local_peer_id));
                info!(address = %dialable, "Escuchando; multiaddr para conexión");
            }
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                info!(%peer_id, "Conexión P2P establecida");
                connected_peers.insert(peer_id);
            }
            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                error!(?peer_id, %error, "Falló conexión P2P saliente");
            }
            SwarmEvent::Behaviour(gossipsub::Event::Subscribed { peer_id, topic }) => {
                info!(%peer_id, topic = topic.as_str(), "Peer remoto suscrito");
                let matches_publication = publication
                    .as_ref()
                    .is_some_and(|pending| pending.topic_hash == topic);
                if matches_publication {
                    let Some(pending) = publication.take() else {
                        bail!("se perdió una publicación pendiente");
                    };
                    swarm
                        .behaviour_mut()
                        .publish(pending.topic, pending.bytes)
                        .with_context(|| {
                            format!("no se pudo publicar WONB {}", pending.block_id)
                        })?;
                    info!(
                        wonb_id = %pending.block_id,
                        topic = topic.as_str(),
                        "WONB válido publicado a peer suscrito"
                    );
                }
            }
            event @ SwarmEvent::Behaviour(gossipsub::Event::Message { .. }) => {
                handle_message_event(event, &mut swarm, &trusted)?;
            }
            event => log_other_event(event),
        }
    }

    loop {
        tokio::select! {
            signal = tokio::signal::ctrl_c() => {
                signal.context("no se pudo esperar Ctrl-C")?;
                info!("Interrumpido por el usuario");
                return Ok(());
            }
            event = swarm.select_next_some() => match event {
                event @ SwarmEvent::Behaviour(gossipsub::Event::Message { .. }) => {
                    handle_message_event(event, &mut swarm, &trusted)?;
                }
                SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                    info!(%peer_id, "Conexión P2P establecida");
                }
                SwarmEvent::ConnectionClosed { peer_id, .. } => {
                    info!(%peer_id, "Conexión P2P cerrada");
                }
                event => log_other_event(event),
            }
        }
    }
}

fn new_swarm() -> Result<NodeSwarm> {
    let gossipsub_config = gossipsub::ConfigBuilder::default()
        .validate_messages()
        .build()
        .map_err(|error| anyhow::anyhow!("configuración GossipSub inválida: {error}"))?;

    let swarm = SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_behaviour(
            |key| -> std::result::Result<_, Box<dyn std::error::Error + Send + Sync>> {
                gossipsub::Behaviour::new(
                    gossipsub::MessageAuthenticity::Signed(key.clone()),
                    gossipsub_config,
                )
                .map_err(|error| {
                    Box::new(std::io::Error::other(error))
                        as Box<dyn std::error::Error + Send + Sync>
                })
            },
        )?
        .build();
    Ok(swarm)
}

fn parse_peer_address(raw: &str) -> Result<(Multiaddr, PeerId)> {
    let address: Multiaddr = raw
        .parse()
        .with_context(|| format!("multiaddr --peer inválida: {raw}"))?;
    let peer_id = match address.iter().last() {
        Some(Protocol::P2p(peer_id)) => peer_id,
        _ => bail!("--peer debe terminar con /p2p/<PEER_ID>: {raw}"),
    };
    Ok((address, peer_id))
}

fn load_trusted_issuers(path: &Path) -> Result<TrustedIssuers> {
    let json = std::fs::read_to_string(path)
        .with_context(|| format!("no se pudo leer trusted issuers: {}", path.display()))?;
    TrustedIssuers::from_json(&json)
        .with_context(|| format!("trusted-issuers inválido: {}", path.display()))
}

fn load_publication(path: &Path, trusted: &TrustedIssuers) -> Result<PendingPublication> {
    let json = std::fs::read_to_string(path)
        .with_context(|| format!("no se pudo leer WONB: {}", path.display()))?;
    let block = validate_block(&json, None, trusted)
        .with_context(|| format!("WONB inválido; no se publicará: {}", path.display()))?;
    let topic = IdentTopic::new(block.body.topic.clone());
    let topic_hash = topic.hash();
    let bytes = block.to_canonical_json()?.into_bytes();
    Ok(PendingPublication {
        topic,
        topic_hash,
        block_id: block.id,
        bytes,
    })
}

fn validate_block(
    payload: &str,
    received_topic: Option<&str>,
    trusted: &TrustedIssuers,
) -> Result<Block> {
    let block = Block::from_json(payload).context("JSON/Bloque WONB 0.2 inválido")?;
    let topic = received_topic.unwrap_or(&block.body.topic);
    if topic != block.body.topic {
        bail!(
            "topic GossipSub no coincide con el WONB: recibido={topic}, declarado={}",
            block.body.topic
        );
    }
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    block
        .verify(Some(&now), trusted)
        .context("verificación de firma, emisor o TTL falló")?;
    verify_chain(std::slice::from_ref(&block)).context("cadena WONB incompleta o inválida")?;
    Ok(block)
}

fn handle_message_event(
    event: SwarmEvent<gossipsub::Event>,
    swarm: &mut NodeSwarm,
    trusted: &TrustedIssuers,
) -> Result<()> {
    let SwarmEvent::Behaviour(gossipsub::Event::Message {
        propagation_source,
        message_id,
        message,
    }) = event
    else {
        return Ok(());
    };

    let topic = message.topic.as_str();
    let payload = std::str::from_utf8(&message.data);
    let result = payload
        .map_err(|error| anyhow::anyhow!("payload no es UTF-8: {error}"))
        .and_then(|payload| validate_block(payload, Some(topic), trusted));

    let acceptance = if result.is_ok() {
        MessageAcceptance::Accept
    } else {
        MessageAcceptance::Reject
    };
    let reported = swarm.behaviour_mut().report_message_validation_result(
        &message_id,
        &propagation_source,
        acceptance,
    );
    if !reported {
        warn!(
            ?message_id,
            "GossipSub ya no tenía pendiente el mensaje validado"
        );
    }

    match result {
        Ok(block) => {
            info!(
                wonb_id = %block.id,
                topic,
                peer = %propagation_source,
                "WONB válido recibido y aceptado"
            );
            Ok(())
        }
        Err(error) => {
            warn!(
                topic,
                peer = %propagation_source,
                %error,
                "WONB rechazado; no se propagará"
            );
            Ok(())
        }
    }
}

fn log_other_event(event: SwarmEvent<gossipsub::Event>) {
    tracing::debug!(?event, "Evento libp2p");
}

#[cfg(test)]
mod tests {
    use std::{
        path::{Path, PathBuf},
        time::Duration,
    };

    use libp2p::{gossipsub, multiaddr::Protocol, swarm::SwarmEvent};
    use serde_json::json;
    use tokio::time::timeout;

    use wombat_core::v02::{
        generate_ephemeral_keypair, issuer_string, Block, BlockType, TrustedIssuer, TrustedIssuers,
    };

    use super::*;

    fn fixture_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../contracts/v1/fixtures/bundle/piura-demo/v0")
    }

    fn fixture_block_and_trusted() -> (String, TrustedIssuers) {
        let root = fixture_dir();
        let block = std::fs::read_to_string(root.join("wonb/change.json")).unwrap();
        let trusted = std::fs::read_to_string(root.join("trusted-issuers.json")).unwrap();
        (block, TrustedIssuers::from_json(&trusted).unwrap())
    }

    fn expired_block_and_trusted() -> (String, TrustedIssuers) {
        let risk_path = fixture_dir().join("wonb/risk/piura-demo-r00c00.json");
        let mut raw: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(risk_path).unwrap()).unwrap();
        let (key, verifying_key) = generate_ephemeral_keypair();
        let issuer = issuer_string(&verifying_key);
        raw["issuer"] = json!(issuer);
        raw["issued_at"] = json!("2000-01-01T00:00:00Z");
        raw["valid_until"] = json!("2000-01-02T00:00:00Z");
        let body = Block::from_value(raw).unwrap().body;
        let block = Block::seal(body, &key).unwrap();
        let trusted = TrustedIssuers {
            version: 1,
            issuers: vec![TrustedIssuer {
                issuer,
                name: "test".into(),
                roles: vec![BlockType::Risk],
            }],
        };
        (block.to_canonical_json().unwrap(), trusted)
    }

    #[test]
    fn invalid_multiaddr_is_rejected() {
        assert!(parse_peer_address("not-a-multiaddr").is_err());
        assert!(parse_peer_address("/ip4/127.0.0.1/tcp/9000").is_err());
    }

    #[test]
    fn valid_signed_fixture_is_accepted() {
        let (json, trusted) = fixture_block_and_trusted();
        let block =
            validate_block(&json, Some("wombat/pe/piura/piura-demo/change"), &trusted).unwrap();
        assert_eq!(
            block.id,
            "sha256:02da13aa3d9cfb4c94ed1512d9728ddb3ed7a945e98c9a3bbd98c9cb791de683"
        );
    }

    #[test]
    fn bad_signature_is_rejected() {
        let (json, trusted) = fixture_block_and_trusted();
        let mut block: serde_json::Value = serde_json::from_str(&json).unwrap();
        block["signature"] = json!("00");
        assert!(validate_block(&block.to_string(), None, &trusted).is_err());
    }

    #[test]
    fn untrusted_issuer_is_rejected() {
        let (json, _) = fixture_block_and_trusted();
        assert!(validate_block(&json, None, &TrustedIssuers::empty()).is_err());
    }

    #[test]
    fn mismatched_topic_is_rejected() {
        let (json, trusted) = fixture_block_and_trusted();
        assert!(validate_block(&json, Some("wombat/pe/piura/piura-demo/risk"), &trusted).is_err());
    }

    #[test]
    fn invalid_json_is_rejected() {
        assert!(validate_block("not json", None, &TrustedIssuers::empty()).is_err());
    }

    #[test]
    fn expired_block_is_rejected() {
        let (json, trusted) = expired_block_and_trusted();
        let error = validate_block(&json, None, &trusted).unwrap_err();
        let error_chain = format!("{error:#}");
        assert!(error_chain.contains("expirado"), "{error_chain}");
    }

    #[tokio::test]
    async fn two_swarms_dial_and_deliver_a_valid_wonb_over_loopback() {
        let (payload, trusted) = fixture_block_and_trusted();
        let block = validate_block(&payload, None, &trusted).unwrap();
        let topic = IdentTopic::new(block.body.topic.clone());

        let mut swarm_a = new_swarm().unwrap();
        let mut swarm_b = new_swarm().unwrap();
        let peer_a = *swarm_a.local_peer_id();
        swarm_a
            .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
            .unwrap();
        swarm_b
            .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
            .unwrap();
        swarm_a.behaviour_mut().subscribe(&topic).unwrap();
        swarm_b.behaviour_mut().subscribe(&topic).unwrap();

        let listen_address = timeout(Duration::from_secs(5), async {
            loop {
                if let SwarmEvent::NewListenAddr { address, .. } = swarm_a.select_next_some().await
                {
                    break address;
                }
            }
        })
        .await
        .expect("A escuchó en loopback");
        let dial_address = listen_address.with(Protocol::P2p(peer_a));
        let (parsed_address, parsed_peer) = parse_peer_address(&dial_address.to_string()).unwrap();
        assert_eq!(parsed_peer, peer_a);
        swarm_b.dial(parsed_address).unwrap();

        let deadline = Duration::from_secs(8);
        let received = timeout(deadline, async {
            let mut published = false;
            loop {
                tokio::select! {
                    event = swarm_a.select_next_some() => {
                        if let SwarmEvent::Behaviour(gossipsub::Event::Subscribed { peer_id, topic: subscribed }) = event {
                            if peer_id != peer_a && subscribed == topic.hash() && !published {
                                swarm_a.behaviour_mut().publish(topic.clone(), payload.as_bytes()).unwrap();
                                published = true;
                            }
                        }
                    }
                    event = swarm_b.select_next_some() => {
                        if let SwarmEvent::Behaviour(gossipsub::Event::Message {
                            propagation_source,
                            message_id,
                            message,
                        }) = event {
                            let source = propagation_source;
                            let result = std::str::from_utf8(&message.data)
                                .map_err(anyhow::Error::from)
                                .and_then(|text| validate_block(text, Some(message.topic.as_str()), &trusted));
                            let acceptance = if result.is_ok() {
                                gossipsub::MessageAcceptance::Accept
                            } else {
                                gossipsub::MessageAcceptance::Reject
                            };
                            let reported = swarm_b.behaviour_mut().report_message_validation_result(
                                &message_id,
                                &propagation_source,
                                acceptance,
                            );
                            assert!(reported);
                            assert_eq!(source, peer_a);
                            break result.unwrap();
                        }
                    }
                }
            }
        })
        .await
        .expect("B recibió el WONB antes del timeout");

        assert_eq!(received.id, block.id);
        assert_eq!(received.body.topic, topic.hash().as_str());
    }
}
