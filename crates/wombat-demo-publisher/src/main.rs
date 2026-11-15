use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use clap::{Parser, ValueEnum};
use ed25519_dalek::SigningKey;
use futures_util::StreamExt;
use libp2p::{
    gossipsub::{self, IdentTopic, TopicHash},
    identity,
    multiaddr::Protocol,
    noise,
    swarm::SwarmEvent,
    tcp, yamux, Multiaddr, PeerId, Swarm, SwarmBuilder,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;
use wombat_core::v02::{
    issuer_string, Block, BlockBody, BlockType, ChangePayload, DataOrigin, Frequency,
    OverlayHashes, Payload, Provenance, Source, TrustedIssuers, WONB_VERSION,
};

type DemoSwarm = Swarm<gossipsub::Behaviour>;

const MAX_BLOCK_SIZE_BYTES: usize = 1024 * 1024;
const PEER_READY_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Scenario {
    #[value(alias = "piura-demo")]
    Piura,
    #[value(alias = "thailand-demo")]
    Thailand,
}

impl Scenario {
    fn as_str(self) -> &'static str {
        match self {
            Self::Piura => "piura",
            Self::Thailand => "thailand",
        }
    }

    fn pipeline_aoi_id(self) -> &'static str {
        match self {
            Self::Piura => "piura-01",
            Self::Thailand => "tailandia-01",
        }
    }

    fn topic_namespace(self) -> &'static str {
        match self {
            Self::Piura => "pe/piura",
            Self::Thailand => "th/thailand",
        }
    }

    fn fallback_aoi_id(self) -> &'static str {
        match self {
            Self::Piura => "piura-demo",
            Self::Thailand => "thailand-demo",
        }
    }

    fn topic(self, aoi_id: &str) -> String {
        match self {
            Self::Piura => format!("wombat/pe/piura/{aoi_id}/change"),
            Self::Thailand => format!("wombat/th/thailand/{aoi_id}/change"),
        }
    }
}

#[derive(Debug, Parser)]
#[command(name = "wombat-demo-publisher")]
#[command(about = "Publica WONBs Change de demo firmados y verificados")]
struct Args {
    #[arg(long, value_enum)]
    scenario: Scenario,

    /// Multiaddr completa que termina en /p2p/<PEER_ID>; no hace falta en dry-run.
    #[arg(long)]
    peer: Option<String>,

    /// Semilla Ed25519 hexadecimal, generada con wombat-cli keygen.
    #[arg(long)]
    key: PathBuf,

    /// Lista externa de emisores de confianza con rol change.
    #[arg(long)]
    trusted_issuers: PathBuf,

    /// Carpeta del pipeline con manifest.json, cells.geojson, change_summary.json y overlays.
    #[arg(long)]
    pipeline_input: Option<PathBuf>,

    #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
    interval_secs: u64,

    #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
    count: u64,

    /// Archivo de identidad libp2p; en modo P2P se crea si no existe.
    #[arg(long)]
    identity_key: Option<PathBuf>,

    /// Genera y verifica los bloques, sin crear una identidad ni usar la red.
    #[arg(long)]
    dry_run: bool,
}

#[derive(Default)]
struct IssuedAtClock {
    last: Option<DateTime<Utc>>,
}

impl IssuedAtClock {
    fn next(&mut self) -> String {
        let mut now = Utc::now();
        if let Some(last) = &self.last {
            if now <= *last {
                now = *last + chrono::Duration::nanoseconds(1);
            }
        }
        self.last = Some(now);
        now.to_rfc3339_opts(SecondsFormat::Nanos, true)
    }
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
}

fn load_signing_key(path: &Path) -> Result<SigningKey> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("no se pudo leer clave de firma: {}", path.display()))?;
    let bytes = hex::decode(raw.trim()).context("la clave de firma no es hexadecimal")?;
    let seed: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("se esperaban 32 bytes de semilla (64 caracteres hex)"))?;
    Ok(SigningKey::from_bytes(&seed))
}

fn load_trusted_issuers(path: &Path) -> Result<TrustedIssuers> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("no se pudo leer trusted issuers: {}", path.display()))?;
    TrustedIssuers::from_json(&raw).context("trusted-issuers inválido")
}

#[derive(Debug, Clone)]
struct ChangeSource {
    aoi_id: String,
    topic: String,
    payload: ChangePayload,
    provenance: Provenance,
    simulation: bool,
}

fn fallback_source(scenario: Scenario) -> ChangeSource {
    let aoi_id = scenario.fallback_aoi_id().to_owned();
    let cell_id = format!("{aoi_id}-simulated-cell");
    let source = Source {
        product: "SIMULATED-WOMBAT-DEMO".to_owned(),
        extra: BTreeMap::from([
            ("cell_id".to_owned(), json!(cell_id)),
            (
                "message".to_owned(),
                json!("DEMO SIMULADA — no es alerta científica"),
            ),
            ("simulation".to_owned(), json!(true)),
        ]),
    };
    ChangeSource {
        topic: scenario.topic(&aoi_id),
        aoi_id,
        payload: ChangePayload {
            aoi_id: scenario.fallback_aoi_id().to_owned(),
            data_origin: DataOrigin::Simulated,
            t1: "2026-10-14T23:00:00Z".to_owned(),
            t2: "2026-10-26T23:00:00Z".to_owned(),
            polarization: "HHHH".to_owned(),
            frequency: Frequency::A,
            pixel_spacing_m: 20,
            delta_db_threshold_cdb: 300,
            aoi_valid_bp: 9000,
            aoi_decrease_bp: 1000,
            aoi_increase_bp: 500,
            overlay_sha256: OverlayHashes {
                t1_png: "11".repeat(32),
                t2_png: "22".repeat(32),
                delta_png: "33".repeat(32),
            },
            cells_geojson_sha256: "44".repeat(32),
        },
        provenance: Provenance {
            sources: vec![source],
            method: "wombat-demo-publisher/simulated/1".to_owned(),
            code_commit: "simulated".to_owned(),
        },
        simulation: true,
    }
}

fn resolve_source(scenario: Scenario, pipeline_input: Option<&Path>) -> ChangeSource {
    match pipeline_input {
        Some(path) => match pipeline_source(scenario, path) {
            Ok(source) => {
                println!("FUENTE: pipeline validado para demo");
                source
            }
            Err(error) => {
                println!("FUENTE: fallback simulado — {error}");
                println!("DEMO SIMULADA — no es alerta científica");
                fallback_source(scenario)
            }
        },
        None => {
            println!("FUENTE: fallback simulado — no se configuró --pipeline-input");
            println!("DEMO SIMULADA — no es alerta científica");
            fallback_source(scenario)
        }
    }
}

fn pipeline_source(scenario: Scenario, directory: &Path) -> Result<ChangeSource> {
    let manifest_path = directory.join("manifest.json");
    let cells_path = directory.join("cells.geojson");
    let summary_path = directory.join("change_summary.json");
    let manifest_bytes = fs::read(&manifest_path)
        .with_context(|| format!("falta manifest.json en {}", directory.display()))?;
    let cells_bytes = fs::read(&cells_path)
        .with_context(|| format!("falta cells.geojson en {}", directory.display()))?;
    let summary_bytes = fs::read(&summary_path)
        .with_context(|| format!("falta change_summary.json en {}", directory.display()))?;
    let manifest: serde_json::Value =
        serde_json::from_slice(&manifest_bytes).context("manifest.json inválido")?;
    let cells: serde_json::Value =
        serde_json::from_slice(&cells_bytes).context("cells.geojson inválido")?;
    let summary: serde_json::Value =
        serde_json::from_slice(&summary_bytes).context("change_summary.json inválido")?;

    validate_manifest(scenario, &manifest)?;
    let crs_compute = required_string(&manifest, "crs_compute", "manifest")?;
    let cell_count = validate_cells(scenario, &cells, crs_compute)?;
    let overlay_dir = directory.join("overlays");
    let overlay_sha256 = OverlayHashes {
        t1_png: sha256_file(&overlay_dir.join("t1.png"))?,
        t2_png: sha256_file(&overlay_dir.join("t2.png"))?,
        delta_png: sha256_file(&overlay_dir.join("delta.png"))?,
    };
    let nisar = manifest
        .get("nisar")
        .context("manifest.nisar falta o no es un objeto")?;
    let t1 = nisar.get("t1").context("manifest.nisar.t1 no existe")?;
    let t2 = nisar.get("t2").context("manifest.nisar.t2 no existe")?;
    let payload = ChangePayload {
        aoi_id: required_string(&manifest, "aoi_id", "manifest")?.to_owned(),
        data_origin: DataOrigin::NisarReal,
        t1: required_string(t1, "datetime", "manifest.nisar.t1")?.to_owned(),
        t2: required_string(t2, "datetime", "manifest.nisar.t2")?.to_owned(),
        polarization: required_string(nisar, "polarization", "manifest.nisar")?.to_owned(),
        frequency: parse_frequency(required_string(nisar, "frequency", "manifest.nisar")?)?,
        pixel_spacing_m: required_u32(nisar, "pixel_spacing_m", "manifest.nisar")?,
        delta_db_threshold_cdb: fraction_or_threshold_cdb(&summary, "delta_db_threshold")?,
        aoi_valid_bp: fraction_to_bp(&summary, "valid_fraction")?,
        aoi_decrease_bp: fraction_to_bp(&summary, "decrease_fraction")?,
        aoi_increase_bp: fraction_to_bp(&summary, "increase_fraction")?,
        overlay_sha256: overlay_sha256.clone(),
        cells_geojson_sha256: sha256_bytes(&cells_bytes),
    };

    let mut extra = BTreeMap::from([
        (
            "aoi_name".to_owned(),
            json!(required_string(&manifest, "aoi_name", "manifest")?),
        ),
        ("cell_count".to_owned(), json!(cell_count)),
        (
            "cells_geojson_sha256".to_owned(),
            json!(payload.cells_geojson_sha256),
        ),
        ("change_summary".to_owned(), summary),
        ("crs_compute".to_owned(), manifest["crs_compute"].clone()),
        ("manifest".to_owned(), manifest.clone()),
        ("manifest_path".to_owned(), json!("manifest.json")),
        (
            "manifest_sha256".to_owned(),
            json!(sha256_bytes(&manifest_bytes)),
        ),
        ("simulation".to_owned(), json!(false)),
        (
            "source_files".to_owned(),
            json!({
                "cells": "cells.geojson",
                "change_summary": "change_summary.json",
                "overlays": {
                    "t1.png": overlay_sha256.t1_png,
                    "t2.png": overlay_sha256.t2_png,
                    "delta.png": overlay_sha256.delta_png
                }
            }),
        ),
    ]);
    if let Some(selected_pair) = manifest.get("selected_pair") {
        extra.insert("selected_pair".to_owned(), selected_pair.clone());
    }
    let product = required_string(nisar, "product", "manifest.nisar")?.to_owned();
    let code_commit = required_string(
        manifest
            .get("pipeline")
            .context("manifest.pipeline no existe")?,
        "commit",
        "manifest.pipeline",
    )?
    .to_owned();
    let aoi_id = payload.aoi_id.clone();
    Ok(ChangeSource {
        topic: scenario.topic(&aoi_id),
        aoi_id,
        payload,
        provenance: Provenance {
            sources: vec![Source { product, extra }],
            method: "wombat-demo-publisher/pipeline-summary/1".to_owned(),
            code_commit,
        },
        simulation: false,
    })
}

fn validate_manifest(scenario: Scenario, manifest: &serde_json::Value) -> Result<()> {
    let aoi_id = required_string(manifest, "aoi_id", "manifest")?;
    if aoi_id != scenario.pipeline_aoi_id() {
        bail!(
            "el AOI del manifest ({aoi_id}) no corresponde al escenario {}",
            scenario.as_str()
        );
    }
    if required_string(manifest, "data_origin", "manifest")? != "nisar_real" {
        bail!("el manifest no declara data_origin nisar_real");
    }
    let crs = required_string(manifest, "crs_compute", "manifest")?;
    if !valid_utm_crs(crs) {
        bail!("manifest.crs_compute no es un CRS UTM WGS84 válido");
    }
    if manifest
        .get("selected_pair")
        .is_some_and(serde_json::Value::is_null)
    {
        bail!("manifest.selected_pair es null");
    }
    let nisar = manifest.get("nisar").context("manifest.nisar no existe")?;
    if nisar
        .get("selected_pair")
        .is_some_and(serde_json::Value::is_null)
    {
        bail!("manifest.nisar.selected_pair es null");
    }
    if nisar.get("comparable").and_then(serde_json::Value::as_bool) != Some(true) {
        bail!("manifest.nisar.comparable no es true");
    }
    let checks = nisar
        .get("comparable_checks")
        .and_then(serde_json::Value::as_object)
        .context("manifest.nisar.comparable_checks falta o no es un objeto")?;
    if checks.is_empty() || checks.values().any(|value| value.as_bool() != Some(true)) {
        bail!("uno o más checks de comparabilidad fallaron o no están presentes");
    }
    for (field, label) in [("t1", "t1"), ("t2", "t2")] {
        let scene = nisar
            .get(field)
            .with_context(|| format!("manifest.nisar.{label} no existe"))?;
        required_string(scene, "datetime", &format!("manifest.nisar.{label}"))?;
        required_string(scene, "granule", &format!("manifest.nisar.{label}"))?;
    }
    required_u32(nisar, "pixel_spacing_m", "manifest.nisar")?;
    required_string(nisar, "polarization", "manifest.nisar")?;
    parse_frequency(required_string(nisar, "frequency", "manifest.nisar")?)?;
    Ok(())
}

fn validate_cells(
    scenario: Scenario,
    cells: &serde_json::Value,
    crs_compute: &str,
) -> Result<usize> {
    if cells.get("type").and_then(serde_json::Value::as_str) != Some("FeatureCollection") {
        bail!("cells.geojson no declara FeatureCollection");
    }
    if let Some(crs_note) = cells.get("crs_note").and_then(serde_json::Value::as_str) {
        if crs_note.contains("calculos hechos en") && !crs_note.contains(crs_compute) {
            bail!("cells.geojson declara un CRS de cálculo distinto al manifest");
        }
    }
    let features = cells
        .get("features")
        .and_then(serde_json::Value::as_array)
        .context("cells.geojson.features falta o no es una lista")?;
    if features.is_empty() {
        bail!("cells.geojson no contiene celdas");
    }
    let required_metrics = [
        "valid_fraction",
        "decrease_fraction",
        "increase_fraction",
        "changed_fraction",
    ];
    for (index, feature) in features.iter().enumerate() {
        let properties = feature
            .get("properties")
            .with_context(|| format!("feature {index} no tiene properties"))?;
        let cell_id = required_string(properties, "cell_id", &format!("feature {index}"))?;
        if !cell_id.starts_with(&format!("{}-", scenario.pipeline_aoi_id())) {
            bail!("cell_id {cell_id} no pertenece al AOI del escenario");
        }
        for field in required_metrics {
            let value = properties
                .get(field)
                .and_then(serde_json::Value::as_f64)
                .with_context(|| format!("feature {index} no contiene {field} numérico"))?;
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                bail!("feature {index}.{field} está fuera del rango 0..=1");
            }
        }
    }
    Ok(features.len())
}

fn required_string<'a>(value: &'a serde_json::Value, key: &str, parent: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .with_context(|| format!("{parent}.{key} falta o no es texto"))
}

fn required_u32(value: &serde_json::Value, key: &str, parent: &str) -> Result<u32> {
    value
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0)
        .with_context(|| format!("{parent}.{key} falta o no es entero positivo"))
}

fn parse_frequency(value: &str) -> Result<Frequency> {
    match value {
        "A" => Ok(Frequency::A),
        "B" => Ok(Frequency::B),
        _ => bail!("frecuencia no soportada por el contrato WONB: {value}"),
    }
}

fn valid_utm_crs(crs: &str) -> bool {
    let Some(zone) = crs.strip_prefix("EPSG:") else {
        return false;
    };
    let Ok(code) = zone.parse::<u16>() else {
        return false;
    };
    (32601..=32660).contains(&code) || (32701..=32760).contains(&code)
}

fn fraction_to_bp(summary: &serde_json::Value, field: &str) -> Result<u32> {
    let value = summary
        .get(field)
        .and_then(serde_json::Value::as_f64)
        .with_context(|| format!("change_summary.{field} falta o no es numérico"))?;
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        bail!("change_summary.{field} está fuera del rango 0..=1");
    }
    Ok((value * 10_000.0).round() as u32)
}

fn fraction_or_threshold_cdb(summary: &serde_json::Value, field: &str) -> Result<u32> {
    let value = summary
        .get(field)
        .and_then(serde_json::Value::as_f64)
        .with_context(|| format!("change_summary.{field} falta o no es numérico"))?;
    if !value.is_finite() || value <= 0.0 || value > f64::from(u32::MAX) / 100.0 {
        bail!("change_summary.{field} está fuera del rango válido");
    }
    Ok((value * 100.0).round() as u32)
}

fn sha256_file(path: &Path) -> Result<String> {
    let bytes =
        fs::read(path).with_context(|| format!("falta archivo de evidencia {}", path.display()))?;
    Ok(sha256_bytes(&bytes))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn build_verified_block(
    scenario: Scenario,
    source: &ChangeSource,
    issued_at: String,
    signing_key: &SigningKey,
    trusted: &TrustedIssuers,
) -> Result<Block> {
    let issuer = issuer_string(&signing_key.verifying_key());
    if !trusted.allows(&issuer, BlockType::Change) {
        bail!("la clave no está habilitada como trusted issuer con rol change");
    }

    let body = BlockBody {
        wonb_version: WONB_VERSION.to_owned(),
        block_type: BlockType::Change,
        topic: source.topic.clone(),
        issued_at: issued_at.clone(),
        valid_until: None,
        issuer,
        parents: Vec::new(),
        payload: Payload::Change(source.payload.clone()),
        provenance: source.provenance.clone(),
    };

    let block = Block::seal(body, signing_key).context("no se pudo firmar el WONB")?;
    block
        .verify(Some(&issued_at), trusted)
        .context("verificación local del WONB firmado falló")?;
    if block.body.block_type != BlockType::Change
        || block.body.topic != source.topic
        || source.aoi_id != source.payload.aoi_id
        || !block.body.topic.starts_with(&format!(
            "wombat/{}/{}/",
            scenario.topic_namespace(),
            source.aoi_id
        ))
        || block.id != block.body.compute_id()?
    {
        bail!("el bloque generado no coincide con el escenario solicitado");
    }
    Ok(block)
}

fn log_block(scenario: Scenario, source: &ChangeSource, block: &Block) {
    info!(
        wonb_id = %block.id,
        aoi_id = %source.aoi_id,
        scenario = scenario.as_str(),
        topic = %block.body.topic,
        simulation = source.simulation,
        data_origin = ?source.payload.data_origin,
        "WONB Change firmado y verificado"
    );
}

fn dry_run(
    args: &Args,
    source: &ChangeSource,
    key: &SigningKey,
    trusted: &TrustedIssuers,
) -> Result<()> {
    let blocks = generate_blocks(args.scenario, source, args.count, key, trusted)?;
    for block in &blocks {
        log_block(args.scenario, source, block);
    }
    info!(
        count = blocks.len(),
        scenario = args.scenario.as_str(),
        "Dry-run completo: todos los ids son distintos; no se abrió la red"
    );
    Ok(())
}

fn generate_blocks(
    scenario: Scenario,
    source: &ChangeSource,
    count: u64,
    key: &SigningKey,
    trusted: &TrustedIssuers,
) -> Result<Vec<Block>> {
    let mut clock = IssuedAtClock::default();
    let mut ids = HashSet::new();
    let mut blocks = Vec::new();
    for _ in 0..count {
        let block = build_verified_block(scenario, source, clock.next(), key, trusted)?;
        if !ids.insert(block.id.clone()) {
            bail!("se generó un id WONB duplicado: {}", block.id);
        }
        blocks.push(block);
    }
    Ok(blocks)
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
                    use std::io::Write;
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

fn new_swarm(identity: identity::Keypair) -> Result<DemoSwarm> {
    let config = gossipsub::ConfigBuilder::default()
        .validate_messages()
        .max_transmit_size(MAX_BLOCK_SIZE_BYTES)
        .build()
        .map_err(|error| anyhow::anyhow!("configuración GossipSub inválida: {error}"))?;
    Ok(SwarmBuilder::with_existing_identity(identity)
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
                    config,
                )
                .map_err(|error| {
                    Box::new(std::io::Error::other(error))
                        as Box<dyn std::error::Error + Send + Sync>
                })
            },
        )?
        .build())
}

async fn wait_until_subscribed(
    swarm: &mut DemoSwarm,
    expected_peer: PeerId,
    expected_topic: &TopicHash,
) -> Result<()> {
    let wait = async {
        let mut connected = false;
        let mut subscribed = false;
        while !connected || !subscribed {
            tokio::select! {
                signal = tokio::signal::ctrl_c() => {
                    signal.context("no se pudo esperar Ctrl-C")?;
                    bail!("interrumpido mientras se esperaba al peer");
                }
                event = swarm.select_next_some() => match event {
                    SwarmEvent::ConnectionEstablished { peer_id, .. } if peer_id == expected_peer => {
                        connected = true;
                        info!(%peer_id, "Conexión P2P establecida");
                    }
                    SwarmEvent::Behaviour(gossipsub::Event::Subscribed { peer_id, topic })
                        if peer_id == expected_peer && &topic == expected_topic =>
                    {
                        subscribed = true;
                        info!(%peer_id, topic = topic.as_str(), "Peer remoto suscrito al topic del escenario");
                    }
                    SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                        warn!(?peer_id, %error, "Falló conexión P2P saliente");
                    }
                    _ => {}
                }
            }
        }
        Ok::<(), anyhow::Error>(())
    };
    tokio::time::timeout(PEER_READY_TIMEOUT, wait)
        .await
        .context("timeout esperando conexión y suscripción P2P real")??;
    Ok(())
}

async fn wait_interval(swarm: &mut DemoSwarm, interval: Duration) -> Result<()> {
    let sleep = tokio::time::sleep(interval);
    tokio::pin!(sleep);
    loop {
        tokio::select! {
            signal = tokio::signal::ctrl_c() => {
                signal.context("no se pudo esperar Ctrl-C")?;
                bail!("interrumpido por el usuario");
            }
            _ = &mut sleep => return Ok(()),
            event = swarm.select_next_some() => {
                if let SwarmEvent::ConnectionClosed { peer_id, .. } = event {
                    warn!(%peer_id, "Conexión P2P cerrada durante el intervalo");
                }
            }
        }
    }
}

async fn publish(
    args: &Args,
    source: &ChangeSource,
    key: &SigningKey,
    trusted: &TrustedIssuers,
) -> Result<()> {
    let peer = args
        .peer
        .as_deref()
        .context("--peer es obligatorio fuera de --dry-run")?;
    let identity_path = args
        .identity_key
        .as_deref()
        .context("--identity-key es obligatorio fuera de --dry-run")?;
    let (address, peer_id) = parse_peer_address(peer)?;
    let mut swarm = new_swarm(load_or_create_identity(identity_path)?)?;
    let topic = IdentTopic::new(source.topic.clone());
    let topic_hash = topic.hash();
    swarm
        .behaviour_mut()
        .subscribe(&topic)
        .context("no se pudo suscribir localmente al topic")?;
    swarm
        .dial(address.clone())
        .with_context(|| format!("no se pudo marcar dial a peer={peer_id} address={address}"))?;
    info!(%peer_id, %address, topic = %source.topic, "Esperando conexión y suscripción del peer remoto");
    wait_until_subscribed(&mut swarm, peer_id, &topic_hash).await?;

    let mut clock = IssuedAtClock::default();
    let mut ids = HashSet::new();
    for index in 0..args.count {
        if index > 0 {
            wait_interval(&mut swarm, Duration::from_secs(args.interval_secs)).await?;
        }
        let block = build_verified_block(args.scenario, source, clock.next(), key, trusted)?;
        if !ids.insert(block.id.clone()) {
            bail!(
                "se generó un id WONB duplicado; publicación detenida: {}",
                block.id
            );
        }
        let bytes = block.to_canonical_json()?.into_bytes();
        swarm
            .behaviour_mut()
            .publish(topic.clone(), bytes)
            .with_context(|| format!("no se pudo publicar WONB {}", block.id))?;
        log_block(args.scenario, source, &block);
    }
    info!(count = ids.len(), "Publicación demo terminada");
    Ok(())
}

async fn run() -> Result<()> {
    let args = Args::parse();
    if args.dry_run && args.peer.is_some() {
        warn!("--peer se ignora en --dry-run; no se abrirá la red");
    }
    let key = load_signing_key(&args.key)?;
    let trusted = load_trusted_issuers(&args.trusted_issuers)?;
    if !trusted.allows(&issuer_string(&key.verifying_key()), BlockType::Change) {
        bail!("la clave no está habilitada como trusted issuer con rol change");
    }
    let source = resolve_source(args.scenario, args.pipeline_input.as_deref());

    if args.dry_run {
        dry_run(&args, &source, &key, &trusted)
    } else {
        publish(&args, &source, &key, &trusted).await
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    run().await
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use wombat_core::v02::{generate_ephemeral_keypair, TrustedIssuer, TrustedIssuers};

    fn signing_key_and_trust() -> (SigningKey, TrustedIssuers) {
        let (key, verifying_key) = generate_ephemeral_keypair();
        let trusted = TrustedIssuers {
            version: 1,
            issuers: vec![TrustedIssuer {
                issuer: issuer_string(&verifying_key),
                name: "test simulated demo".to_owned(),
                roles: vec![BlockType::Change],
            }],
        };
        (key, trusted)
    }

    fn write_pipeline_fixture(directory: &Path, scenario: Scenario) {
        fs::create_dir_all(directory.join("overlays")).unwrap();
        let aoi_id = scenario.pipeline_aoi_id();
        let crs = if scenario == Scenario::Piura {
            "EPSG:32717"
        } else {
            "EPSG:32647"
        };
        let manifest = json!({
            "aoi_id": aoi_id,
            "aoi_name": format!("Test {aoi_id}"),
            "data_origin": "nisar_real",
            "crs_compute": crs,
            "nisar": {
                "product": "NISAR_L2_GCOV",
                "maturity": "PROVISIONAL",
                "frame": "081",
                "orbit_direction": "descending",
                "mode": "4005",
                "frequency": "A",
                "polarization": "HHHH",
                "pixel_spacing_m": 20,
                "t1": {"granule": "granule-t1", "datetime": "2026-10-14T23:00:00Z"},
                "t2": {"granule": "granule-t2", "datetime": "2026-10-26T23:00:00Z"},
                "comparable": true,
                "comparable_checks": {
                    "same_frame": true,
                    "same_orbit_direction": true,
                    "same_mode": true,
                    "same_frequency": true,
                    "same_polarization": true,
                    "same_maturity": true
                }
            },
            "pipeline": {"commit": "abc123", "risk_config_version": "risk-config/test"}
        });
        let cells = json!({
            "type": "FeatureCollection",
            "crs_note": format!("geometry en EPSG:4326; calculos hechos en {crs}"),
            "features": [{
                "type": "Feature",
                "properties": {
                    "cell_id": format!("{aoi_id}-r00c00"),
                    "valid_fraction": 0.8,
                    "decrease_fraction": 0.2,
                    "increase_fraction": 0.1,
                    "changed_fraction": 0.3
                }
            }]
        });
        let summary = json!({
            "valid_fraction": 0.8,
            "decrease_fraction": 0.2,
            "increase_fraction": 0.1,
            "delta_db_threshold": 3.0
        });
        fs::write(
            directory.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        fs::write(
            directory.join("cells.geojson"),
            serde_json::to_vec(&cells).unwrap(),
        )
        .unwrap();
        fs::write(
            directory.join("change_summary.json"),
            serde_json::to_vec(&summary).unwrap(),
        )
        .unwrap();
        for name in ["t1.png", "t2.png", "delta.png"] {
            fs::write(
                directory.join("overlays").join(name),
                format!("test-{name}"),
            )
            .unwrap();
        }
    }

    #[test]
    fn piura_dry_run_blocks_have_distinct_ids_and_correct_topic() {
        let (key, trusted) = signing_key_and_trust();
        let source = fallback_source(Scenario::Piura);
        let blocks = generate_blocks(Scenario::Piura, &source, 2, &key, &trusted).unwrap();
        let first = &blocks[0];
        let second = &blocks[1];

        assert_ne!(first.id, second.id);
        assert_ne!(first.body.issued_at, second.body.issued_at);
        assert_eq!(first.body.topic, "wombat/pe/piura/piura-demo/change");
        assert_eq!(first.body.block_type, BlockType::Change);
        let Payload::Change(payload) = &first.body.payload else {
            panic!("el demo publisher solo debe emitir Change");
        };
        assert_eq!(payload.aoi_id, "piura-demo");
        assert_eq!(payload.data_origin, DataOrigin::Simulated);
        assert!(source.simulation);
        assert_eq!(source.provenance.sources[0].extra["simulation"], true);
        assert!(first.body.provenance.sources[0].extra["cell_id"]
            .as_str()
            .unwrap()
            .starts_with("piura-demo-"));
        for block in &blocks {
            block.verify(Some(&block.body.issued_at), &trusted).unwrap();
        }
    }

    #[test]
    fn thailand_dry_run_blocks_have_distinct_ids_and_isolated_topic() {
        let (key, trusted) = signing_key_and_trust();
        let source = fallback_source(Scenario::Thailand);
        let blocks = generate_blocks(Scenario::Thailand, &source, 2, &key, &trusted).unwrap();
        let first = &blocks[0];
        let second = &blocks[1];

        assert_ne!(first.id, second.id);
        assert_ne!(first.body.issued_at, second.body.issued_at);
        assert_eq!(first.body.topic, "wombat/th/thailand/thailand-demo/change");
        assert!(!first.body.topic.contains("piura"));
        assert_eq!(first.body.block_type, BlockType::Change);
        let Payload::Change(payload) = &first.body.payload else {
            panic!("el demo publisher solo debe emitir Change");
        };
        assert_eq!(payload.aoi_id, "thailand-demo");
        assert_eq!(payload.data_origin, DataOrigin::Simulated);
        assert!(first.body.provenance.sources[0].extra["cell_id"]
            .as_str()
            .unwrap()
            .starts_with("thailand-demo-"));
    }

    #[test]
    fn generated_blocks_cannot_be_risk_or_action() {
        let (key, trusted) = signing_key_and_trust();
        for scenario in [Scenario::Piura, Scenario::Thailand] {
            let source = fallback_source(scenario);
            let blocks = generate_blocks(scenario, &source, 2, &key, &trusted).unwrap();
            assert!(blocks
                .iter()
                .all(|block| block.body.block_type == BlockType::Change));
        }
    }

    #[test]
    fn dry_run_needs_no_peer_or_transport_identity() {
        let (key, trusted) = signing_key_and_trust();
        let source = fallback_source(Scenario::Piura);
        let args = Args {
            scenario: Scenario::Piura,
            peer: None,
            key: PathBuf::from("unused-private-key-path"),
            trusted_issuers: PathBuf::from("unused-trusted-issuers-path"),
            pipeline_input: None,
            interval_secs: 1,
            count: 3,
            identity_key: None,
            dry_run: true,
        };
        dry_run(&args, &source, &key, &trusted).unwrap();
    }

    #[test]
    fn valid_pipeline_builds_signed_real_change_with_provenance() {
        let (key, trusted) = signing_key_and_trust();
        let directory = tempfile::tempdir().unwrap();
        write_pipeline_fixture(directory.path(), Scenario::Piura);
        let source = pipeline_source(Scenario::Piura, directory.path()).unwrap();

        assert!(!source.simulation);
        assert_eq!(source.aoi_id, "piura-01");
        assert_eq!(source.topic, "wombat/pe/piura/piura-01/change");
        assert_eq!(source.payload.data_origin, DataOrigin::NisarReal);
        assert_eq!(source.provenance.sources[0].extra["simulation"], false);
        assert_eq!(
            source.provenance.sources[0].extra["crs_compute"],
            "EPSG:32717"
        );
        assert_eq!(source.provenance.sources[0].extra["cell_count"], 1);
        assert_eq!(
            source.provenance.sources[0].extra["manifest"]["nisar"]["t1"]["granule"],
            "granule-t1"
        );

        let block = build_verified_block(
            Scenario::Piura,
            &source,
            "2026-11-15T19:00:00Z".to_owned(),
            &key,
            &trusted,
        )
        .unwrap();
        block.verify(Some(&block.body.issued_at), &trusted).unwrap();
        assert_eq!(block.body.topic, "wombat/pe/piura/piura-01/change");
    }

    #[test]
    fn non_comparable_pipeline_uses_simulated_fallback() {
        let directory = tempfile::tempdir().unwrap();
        write_pipeline_fixture(directory.path(), Scenario::Piura);
        let manifest_path = directory.path().join("manifest.json");
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest["nisar"]["comparable"] = json!(false);
        fs::write(manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();

        let source = resolve_source(Scenario::Piura, Some(directory.path()));
        assert!(source.simulation);
        assert_eq!(source.payload.data_origin, DataOrigin::Simulated);
    }

    #[test]
    fn missing_pipeline_files_use_simulated_fallback() {
        let directory = tempfile::tempdir().unwrap();
        let source = resolve_source(Scenario::Thailand, Some(directory.path()));
        assert!(source.simulation);
        assert_eq!(source.aoi_id, "thailand-demo");
    }

    #[test]
    fn invalid_cells_use_simulated_fallback() {
        let directory = tempfile::tempdir().unwrap();
        write_pipeline_fixture(directory.path(), Scenario::Thailand);
        fs::write(
            directory.path().join("cells.geojson"),
            r#"{"type":"FeatureCollection","features":[{"type":"Feature","properties":{"cell_id":"piura-01-r00c00"}}]}"#,
        )
        .unwrap();

        let source = resolve_source(Scenario::Thailand, Some(directory.path()));
        assert!(source.simulation);
        assert_eq!(source.aoi_id, "thailand-demo");
        assert_eq!(source.topic, "wombat/th/thailand/thailand-demo/change");
    }

    #[test]
    fn valid_thailand_pipeline_stays_on_its_own_aoi_and_topic() {
        let directory = tempfile::tempdir().unwrap();
        write_pipeline_fixture(directory.path(), Scenario::Thailand);
        let source = pipeline_source(Scenario::Thailand, directory.path()).unwrap();
        assert_eq!(source.aoi_id, "tailandia-01");
        assert_eq!(source.topic, "wombat/th/thailand/tailandia-01/change");
        assert!(!source.topic.contains("piura"));
    }

    #[test]
    fn fallback_does_not_claim_nisar_origin_or_scientific_evidence() {
        for scenario in [Scenario::Piura, Scenario::Thailand] {
            let source = fallback_source(scenario);
            let block_json = serde_json::to_string(&json!({
                "payload": source.payload,
                "provenance": source.provenance,
                "simulation": source.simulation
            }))
            .unwrap();
            assert!(source.simulation);
            assert_eq!(source.payload.data_origin, DataOrigin::Simulated);
            assert!(!block_json.contains("nisar_real"));
            assert!(!block_json.contains("scientific evidence"));
            assert!(block_json.contains("DEMO SIMULADA — no es alerta científica"));
            assert!(block_json.contains("DEMO SIMULADA — no es alerta científica"));
        }
    }
}
