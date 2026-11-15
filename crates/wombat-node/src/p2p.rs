use std::{
    collections::{HashMap, HashSet},
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use chrono::{SecondsFormat, Utc};
use futures_util::StreamExt;
use libp2p::{
    gossipsub::{self, IdentTopic, MessageAcceptance, TopicHash},
    identity,
    multiaddr::Protocol,
    noise,
    swarm::SwarmEvent,
    tcp, yamux, Multiaddr, PeerId, Swarm, SwarmBuilder,
};
use tracing::{error, info, warn};
use wombat_core::v02::{verify_chain, Block, BlockType, TrustedIssuers};

use crate::config::NodeConfig;

type NodeSwarm = Swarm<gossipsub::Behaviour>;

const MAX_BLOCK_SIZE_BYTES: usize = 1024 * 1024;
const MAX_CACHED_BLOCKS: usize = 1024;
const MAX_CACHE_SIZE_BYTES: usize = 16 * 1024 * 1024;
const PENDING_BLOCK_TTL: Duration = Duration::from_secs(10 * 60);
const READY_BLOCK_TTL: Duration = Duration::from_secs(60 * 60);
const CACHE_CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

struct PendingPublication {
    topic: IdentTopic,
    topic_hash: TopicHash,
    block_id: String,
    bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockReadiness {
    Pending,
    Ready,
}

struct CachedBlock {
    block: Block,
    readiness: BlockReadiness,
    inserted_at: Instant,
    size_bytes: usize,
}

struct PendingBlockCache {
    entries: HashMap<String, CachedBlock>,
    size_bytes: usize,
    max_entries: usize,
    max_size_bytes: usize,
    pending_ttl: Duration,
    ready_ttl: Duration,
}

impl PendingBlockCache {
    fn new() -> Self {
        Self::with_limits(
            MAX_CACHED_BLOCKS,
            MAX_CACHE_SIZE_BYTES,
            PENDING_BLOCK_TTL,
            READY_BLOCK_TTL,
        )
    }

    fn with_limits(
        max_entries: usize,
        max_size_bytes: usize,
        pending_ttl: Duration,
        ready_ttl: Duration,
    ) -> Self {
        Self {
            entries: HashMap::new(),
            size_bytes: 0,
            max_entries,
            max_size_bytes,
            pending_ttl,
            ready_ttl,
        }
    }

    fn insert(&mut self, block: Block, size_bytes: usize, now: Instant) -> Result<()> {
        if size_bytes > self.max_size_bytes {
            bail!(
                "WONB excede el límite de caché de {} bytes",
                self.max_size_bytes
            );
        }
        if self.entries.contains_key(&block.id) {
            return Ok(());
        }
        if self.max_entries == 0 || self.max_size_bytes == 0 {
            bail!("caché P2P deshabilitada por sus límites");
        }

        self.cleanup(now);
        while self.entries.len() >= self.max_entries
            || self.size_bytes.saturating_add(size_bytes) > self.max_size_bytes
        {
            let oldest_id = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| {
                    (entry.readiness == BlockReadiness::Ready, entry.inserted_at)
                })
                .map(|(id, _)| id.clone())
                .context("no se pudo liberar espacio en la caché P2P")?;
            self.remove(&oldest_id);
        }

        let id = block.id.clone();
        self.entries.insert(
            id,
            CachedBlock {
                block,
                readiness: BlockReadiness::Pending,
                inserted_at: now,
                size_bytes,
            },
        );
        self.size_bytes += size_bytes;
        Ok(())
    }

    fn remove(&mut self, id: &str) -> Option<CachedBlock> {
        let entry = self.entries.remove(id)?;
        self.size_bytes = self.size_bytes.saturating_sub(entry.size_bytes);
        Some(entry)
    }

    fn cleanup(&mut self, now: Instant) -> usize {
        let expired_ids: Vec<_> = self
            .entries
            .iter()
            .filter_map(|(id, entry)| {
                let ttl = match entry.readiness {
                    BlockReadiness::Pending => self.pending_ttl,
                    BlockReadiness::Ready => self.ready_ttl,
                };
                (now.saturating_duration_since(entry.inserted_at) >= ttl).then(|| id.clone())
            })
            .collect();
        let removed = expired_ids.len();
        for id in expired_ids {
            self.remove(&id);
        }
        removed
    }

    fn chain_readiness(&self, id: &str) -> ChainReadiness {
        let mut pending = vec![id.to_owned()];
        let mut visited = HashSet::new();
        let mut chain = Vec::new();

        while let Some(current_id) = pending.pop() {
            if !visited.insert(current_id.clone()) {
                continue;
            }
            let Some(entry) = self.entries.get(&current_id) else {
                return ChainReadiness::Pending;
            };
            pending.extend(entry.block.body.parents.iter().cloned());
            chain.push(entry.block.clone());
        }

        match verify_chain(&chain) {
            Ok(()) => ChainReadiness::Ready,
            Err(error) => ChainReadiness::Invalid(error.to_string()),
        }
    }

    fn resolve_chains(&mut self) -> (Vec<String>, Vec<(String, String)>) {
        let ids: Vec<_> = self.entries.keys().cloned().collect();
        let mut newly_ready = Vec::new();
        let mut invalid = Vec::new();

        for id in ids {
            match self.chain_readiness(&id) {
                ChainReadiness::Pending => {}
                ChainReadiness::Ready => {
                    if let Some(entry) = self.entries.get_mut(&id) {
                        if entry.readiness == BlockReadiness::Pending {
                            entry.readiness = BlockReadiness::Ready;
                            newly_ready.push(id);
                        }
                    }
                }
                ChainReadiness::Invalid(error) => invalid.push((id, error)),
            }
        }

        for (id, _) in &invalid {
            self.remove(id);
        }
        (newly_ready, invalid)
    }

    fn readiness(&self, id: &str) -> Option<BlockReadiness> {
        self.entries.get(id).map(|entry| entry.readiness)
    }
}

enum ChainReadiness {
    Pending,
    Ready,
    Invalid(String),
}

pub async fn run(config: NodeConfig) -> Result<()> {
    let trusted_path = config
        .trusted_issuers
        .as_deref()
        .context("--trusted-issuers es obligatorio para el modo P2P")?;
    let trusted = load_trusted_issuers(trusted_path)?;
    let identity = match config.identity_key.as_deref() {
        Some(path) => Some(load_or_create_identity(path)?),
        None => {
            warn!(
                "No se especificó --identity-key; se usará una identidad efímera y el PeerId cambiará al reiniciar"
            );
            None
        }
    };

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

    let mut swarm = new_swarm(identity)?;
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
    let mut pending_blocks = PendingBlockCache::new();
    let mut cleanup_interval = tokio::time::interval(CACHE_CLEANUP_INTERVAL);

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
            _ = cleanup_interval.tick() => {
                let removed = pending_blocks.cleanup(Instant::now());
                if removed > 0 {
                    info!(removed, "Bloques vencidos eliminados de la caché P2P");
                }
                continue;
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
                handle_message_event(event, &mut swarm, &trusted, &mut pending_blocks)?;
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
            _ = cleanup_interval.tick() => {
                let removed = pending_blocks.cleanup(Instant::now());
                if removed > 0 {
                    info!(removed, "Bloques vencidos eliminados de la caché P2P");
                }
            }
            event = swarm.select_next_some() => match event {
                event @ SwarmEvent::Behaviour(gossipsub::Event::Message { .. }) => {
                    handle_message_event(event, &mut swarm, &trusted, &mut pending_blocks)?;
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

fn new_swarm(identity: Option<identity::Keypair>) -> Result<NodeSwarm> {
    let gossipsub_config = gossipsub::ConfigBuilder::default()
        .validate_messages()
        .max_transmit_size(MAX_BLOCK_SIZE_BYTES)
        .build()
        .map_err(|error| anyhow::anyhow!("configuración GossipSub inválida: {error}"))?;

    let builder = identity.map_or_else(
        SwarmBuilder::with_new_identity,
        SwarmBuilder::with_existing_identity,
    );
    let swarm = builder
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

fn load_or_create_identity(path: &Path) -> Result<identity::Keypair> {
    match fs::read(path) {
        Ok(bytes) => identity::Keypair::from_protobuf_encoding(&bytes)
            .with_context(|| format!("identidad libp2p inválida: {}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let keypair = identity::Keypair::generate_ed25519();
            let encoded = keypair
                .to_protobuf_encoding()
                .context("no se pudo serializar la identidad libp2p")?;
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }

            match options.open(path) {
                Ok(mut file) => {
                    file.write_all(&encoded).with_context(|| {
                        format!("no se pudo guardar identidad libp2p: {}", path.display())
                    })?;
                    file.sync_all().with_context(|| {
                        format!(
                            "no se pudo sincronizar identidad libp2p: {}",
                            path.display()
                        )
                    })?;
                    Ok(keypair)
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    let bytes = fs::read(path).with_context(|| {
                        format!("no se pudo leer identidad libp2p: {}", path.display())
                    })?;
                    identity::Keypair::from_protobuf_encoding(&bytes)
                        .with_context(|| format!("identidad libp2p inválida: {}", path.display()))
                }
                Err(error) => Err(error).with_context(|| {
                    format!("no se pudo crear identidad libp2p: {}", path.display())
                }),
            }
        }
        Err(error) => Err(error)
            .with_context(|| format!("no se pudo leer identidad libp2p: {}", path.display())),
    }
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
    let block = validate_individual_block(&json, None, trusted)
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

fn validate_individual_block(
    payload: &str,
    received_topic: Option<&str>,
    trusted: &TrustedIssuers,
) -> Result<Block> {
    if payload.len() > MAX_BLOCK_SIZE_BYTES {
        bail!(
            "WONB excede el límite de mensaje de {} bytes",
            MAX_BLOCK_SIZE_BYTES
        );
    }
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
    validate_parent_count(&block)?;
    Ok(block)
}

fn validate_parent_count(block: &Block) -> Result<()> {
    let parents = &block.body.parents;
    let valid = match block.body.block_type {
        BlockType::Change | BlockType::Context => parents.is_empty(),
        BlockType::Risk => parents.len() == 2 && parents[0] != parents[1],
        BlockType::Action => parents.len() == 1,
    };
    if !valid {
        bail!(
            "cantidad o identidad de padres inválida para el bloque {} {}",
            block.body.block_type,
            block.id
        );
    }
    Ok(())
}

fn handle_message_event(
    event: SwarmEvent<gossipsub::Event>,
    swarm: &mut NodeSwarm,
    trusted: &TrustedIssuers,
    pending_blocks: &mut PendingBlockCache,
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
        .and_then(|payload| {
            receive_block(
                payload,
                Some(topic),
                message.data.len(),
                trusted,
                pending_blocks,
            )
        });

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
        Ok((block, readiness, newly_ready)) => {
            info!(
                wonb_id = %block.id,
                topic,
                peer = %propagation_source,
                readiness = ?readiness,
                "WONB individualmente válido recibido y aceptado"
            );
            if readiness == BlockReadiness::Pending {
                info!(
                    wonb_id = %block.id,
                    parents = ?block.body.parents,
                    "WONB propagado y retenido como pendiente; no está listo para consumidores"
                );
            }
            for ready_id in newly_ready {
                info!(
                    wonb_id = %ready_id,
                    "Cadena WONB completa y verificada; bloque listo para consumidores"
                );
            }
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

fn receive_block(
    payload: &str,
    received_topic: Option<&str>,
    size_bytes: usize,
    trusted: &TrustedIssuers,
    pending_blocks: &mut PendingBlockCache,
) -> Result<(Block, BlockReadiness, Vec<String>)> {
    let block = validate_individual_block(payload, received_topic, trusted)?;
    let id = block.id.clone();
    pending_blocks.insert(block.clone(), size_bytes, Instant::now())?;
    match pending_blocks.chain_readiness(&id) {
        ChainReadiness::Invalid(error) => {
            pending_blocks.remove(&id);
            bail!("cadena WONB inválida: {error}");
        }
        ChainReadiness::Pending | ChainReadiness::Ready => {}
    }

    let (newly_ready, invalid) = pending_blocks.resolve_chains();
    for (invalid_id, error) in invalid {
        warn!(
            wonb_id = %invalid_id,
            %error,
            "WONB pendiente descartado al resolverse su cadena"
        );
    }
    let readiness = pending_blocks
        .readiness(&id)
        .context("el WONB desapareció de la caché durante la verificación")?;
    Ok((block, readiness, newly_ready))
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
        generate_ephemeral_keypair, issuer_string, valid_until_from, Block, BlockBody, BlockType,
        TrustedIssuer, TrustedIssuers,
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

    fn signed_fixture_block(
        relative_path: &str,
        issuer: &str,
        parents: Vec<String>,
        issued_at: &str,
        seal: impl FnOnce(BlockBody) -> Block,
    ) -> Block {
        let path = fixture_dir().join(relative_path);
        let mut raw: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        raw["issuer"] = json!(issuer);
        raw["issued_at"] = json!(issued_at);
        raw["parents"] = json!(parents);
        let block_type = raw["type"].as_str().unwrap();
        raw["valid_until"] = match block_type {
            "risk" => json!(valid_until_from(issued_at, 24 * 60 * 60).unwrap()),
            "action" => json!(valid_until_from(issued_at, 60 * 60).unwrap()),
            _ => serde_json::Value::Null,
        };
        seal(Block::from_value(raw).unwrap().body)
    }

    fn signed_chain() -> (Block, Block, Block, Block, TrustedIssuers) {
        let (key, verifying_key) = generate_ephemeral_keypair();
        let issuer = issuer_string(&verifying_key);
        let issued_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
        let change = signed_fixture_block(
            "wonb/change.json",
            &issuer,
            Vec::new(),
            &issued_at,
            |body| Block::seal(body, &key).unwrap(),
        );
        let context = signed_fixture_block(
            "wonb/context.json",
            &issuer,
            Vec::new(),
            &issued_at,
            |body| Block::seal(body, &key).unwrap(),
        );
        let risk = signed_fixture_block(
            "wonb/risk/piura-demo-r00c00.json",
            &issuer,
            vec![change.id.clone(), context.id.clone()],
            &issued_at,
            |body| Block::seal(body, &key).unwrap(),
        );
        let action = signed_fixture_block(
            "wonb/action/1.json",
            &issuer,
            vec![risk.id.clone()],
            &issued_at,
            |body| Block::seal(body, &key).unwrap(),
        );
        let trusted = TrustedIssuers {
            version: 1,
            issuers: vec![TrustedIssuer {
                issuer,
                name: "test".into(),
                roles: vec![
                    BlockType::Change,
                    BlockType::Context,
                    BlockType::Risk,
                    BlockType::Action,
                ],
            }],
        };
        (change, context, risk, action, trusted)
    }

    fn deliver(
        block: &Block,
        trusted: &TrustedIssuers,
        cache: &mut PendingBlockCache,
    ) -> Result<BlockReadiness> {
        let payload = block.to_canonical_json()?;
        receive_block(
            &payload,
            Some(&block.body.topic),
            payload.len(),
            trusted,
            cache,
        )
        .map(|(_, readiness, _)| readiness)
    }

    #[test]
    fn invalid_multiaddr_is_rejected() {
        assert!(parse_peer_address("not-a-multiaddr").is_err());
        assert!(parse_peer_address("/ip4/127.0.0.1/tcp/9000").is_err());
    }

    #[test]
    fn identity_key_preserves_peer_id_between_starts() {
        let directory = tempfile::tempdir().unwrap();
        let identity_path = directory.path().join("transport.key");

        let first_start = load_or_create_identity(&identity_path).unwrap();
        let second_start = load_or_create_identity(&identity_path).unwrap();

        assert_eq!(
            first_start.public().to_peer_id(),
            second_start.public().to_peer_id()
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mode = fs::metadata(identity_path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn valid_signed_fixture_is_accepted() {
        let (json, trusted) = fixture_block_and_trusted();
        let block =
            validate_individual_block(&json, Some("wombat/pe/piura/piura-demo/change"), &trusted)
                .unwrap();
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
        let mut cache = PendingBlockCache::new();
        assert!(receive_block(
            &block.to_string(),
            None,
            block.to_string().len(),
            &trusted,
            &mut cache
        )
        .is_err());
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn untrusted_issuer_is_rejected() {
        let (json, _) = fixture_block_and_trusted();
        let mut cache = PendingBlockCache::new();
        assert!(receive_block(
            &json,
            None,
            json.len(),
            &TrustedIssuers::empty(),
            &mut cache
        )
        .is_err());
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn mismatched_topic_is_rejected() {
        let (json, trusted) = fixture_block_and_trusted();
        let mut cache = PendingBlockCache::new();
        assert!(receive_block(
            &json,
            Some("wombat/pe/piura/piura-demo/risk"),
            json.len(),
            &trusted,
            &mut cache
        )
        .is_err());
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn risk_with_duplicate_parent_ids_is_rejected_before_propagation() {
        let (key, verifying_key) = generate_ephemeral_keypair();
        let issuer = issuer_string(&verifying_key);
        let issued_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
        let repeated_parent = format!("sha256:{}", "00".repeat(32));
        let risk = signed_fixture_block(
            "wonb/risk/piura-demo-r00c00.json",
            &issuer,
            vec![repeated_parent.clone(), repeated_parent],
            &issued_at,
            |body| Block::seal(body, &key).unwrap(),
        );
        let trusted = TrustedIssuers {
            version: 1,
            issuers: vec![TrustedIssuer {
                issuer,
                name: "test".into(),
                roles: vec![BlockType::Risk],
            }],
        };
        let payload = risk.to_canonical_json().unwrap();
        let mut cache = PendingBlockCache::new();

        assert!(receive_block(
            &payload,
            Some(&risk.body.topic),
            payload.len(),
            &trusted,
            &mut cache
        )
        .is_err());
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn invalid_json_is_rejected() {
        assert!(validate_individual_block("not json", None, &TrustedIssuers::empty()).is_err());
    }

    #[test]
    fn expired_block_is_rejected() {
        let (json, trusted) = expired_block_and_trusted();
        let mut cache = PendingBlockCache::new();
        let error = receive_block(&json, None, json.len(), &trusted, &mut cache).unwrap_err();
        let error_chain = format!("{error:#}");
        assert!(error_chain.contains("expirado"), "{error_chain}");
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn risk_becomes_ready_when_separate_change_and_context_arrive_in_order() {
        let (change, context, risk, _, trusted) = signed_chain();
        let mut cache = PendingBlockCache::new();

        assert_eq!(
            deliver(&change, &trusted, &mut cache).unwrap(),
            BlockReadiness::Ready
        );
        assert_eq!(
            deliver(&context, &trusted, &mut cache).unwrap(),
            BlockReadiness::Ready
        );
        assert_eq!(
            deliver(&risk, &trusted, &mut cache).unwrap(),
            BlockReadiness::Ready
        );
        assert_eq!(cache.readiness(&risk.id), Some(BlockReadiness::Ready));
    }

    #[test]
    fn risk_received_before_its_parents_is_pending_until_both_arrive() {
        let (change, context, risk, _, trusted) = signed_chain();
        let mut cache = PendingBlockCache::new();

        assert_eq!(
            deliver(&risk, &trusted, &mut cache).unwrap(),
            BlockReadiness::Pending
        );
        assert_eq!(
            deliver(&change, &trusted, &mut cache).unwrap(),
            BlockReadiness::Ready
        );
        assert_eq!(cache.readiness(&risk.id), Some(BlockReadiness::Pending));
        assert_eq!(
            deliver(&context, &trusted, &mut cache).unwrap(),
            BlockReadiness::Ready
        );
        assert_eq!(cache.readiness(&risk.id), Some(BlockReadiness::Ready));
    }

    #[test]
    fn action_received_before_risk_and_roots_becomes_ready_after_full_chain() {
        let (change, context, risk, action, trusted) = signed_chain();
        let mut cache = PendingBlockCache::new();

        assert_eq!(
            deliver(&action, &trusted, &mut cache).unwrap(),
            BlockReadiness::Pending
        );
        assert_eq!(
            deliver(&risk, &trusted, &mut cache).unwrap(),
            BlockReadiness::Pending
        );
        assert_eq!(
            deliver(&change, &trusted, &mut cache).unwrap(),
            BlockReadiness::Ready
        );
        assert_eq!(
            deliver(&context, &trusted, &mut cache).unwrap(),
            BlockReadiness::Ready
        );
        assert_eq!(cache.readiness(&risk.id), Some(BlockReadiness::Ready));
        assert_eq!(cache.readiness(&action.id), Some(BlockReadiness::Ready));
    }

    #[test]
    fn cache_is_bounded_and_expires_pending_blocks() {
        let (change, context, risk, _, trusted) = signed_chain();
        let payload = change.to_canonical_json().unwrap();
        let mut undersized = PendingBlockCache::with_limits(
            2,
            payload.len() - 1,
            PENDING_BLOCK_TTL,
            READY_BLOCK_TTL,
        );
        assert!(receive_block(
            &payload,
            Some(&change.body.topic),
            payload.len(),
            &trusted,
            &mut undersized
        )
        .is_err());
        assert!(undersized.entries.is_empty());

        let mut bounded = PendingBlockCache::with_limits(
            2,
            MAX_BLOCK_SIZE_BYTES * 2,
            PENDING_BLOCK_TTL,
            READY_BLOCK_TTL,
        );
        deliver(&change, &trusted, &mut bounded).unwrap();
        deliver(&context, &trusted, &mut bounded).unwrap();
        deliver(&risk, &trusted, &mut bounded).unwrap();
        assert!(bounded.entries.len() <= 2);
        assert!(bounded.size_bytes <= bounded.max_size_bytes);

        let mut expiring = PendingBlockCache::with_limits(
            2,
            MAX_BLOCK_SIZE_BYTES * 2,
            Duration::from_secs(10),
            READY_BLOCK_TTL,
        );
        assert_eq!(
            deliver(&risk, &trusted, &mut expiring).unwrap(),
            BlockReadiness::Pending
        );
        let inserted_at = expiring.entries.get(&risk.id).unwrap().inserted_at;
        assert_eq!(expiring.cleanup(inserted_at + Duration::from_secs(10)), 1);
        assert!(expiring.entries.is_empty());
        assert_eq!(expiring.size_bytes, 0);
    }

    #[tokio::test]
    async fn two_swarms_dial_and_deliver_a_valid_wonb_over_loopback() {
        let (payload, trusted) = fixture_block_and_trusted();
        let block = validate_individual_block(&payload, None, &trusted).unwrap();
        let topic = IdentTopic::new(block.body.topic.clone());

        let mut swarm_a = new_swarm(None).unwrap();
        let mut swarm_b = new_swarm(None).unwrap();
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
                                .and_then(|text| validate_individual_block(text, Some(message.topic.as_str()), &trusted));
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
