//! Configuración del nodo WOMBAT.
//!
//! Define los topics por defecto a los que se suscribe el nodo y los
//! parámetros de arranque del transporte libp2p.

use clap::Parser;
use std::path::PathBuf;

/// Topics por defecto del MVP WOMBAT.
#[allow(dead_code)]
pub const DEFAULT_TOPICS: &[&str] = &[
    "wombat/lima/costa-verde/anomaly",
    "wombat/lima/costa-verde/context",
    "wombat/lima/costa-verde/risk",
    "wombat/lima/costa-verde/action",
];

/// Prefijo obligatorio de todos los topics WOMBAT.
#[allow(dead_code)]
pub const TOPIC_PREFIX: &str = "wombat/";

/// Subcomandos soportados por la CLI del nodo.
#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// Ejecuta el nodo en modo demo o P2P (este último requiere la feature p2p).
    Run {
        /// Escuchar en esta dirección multiaddr.
        #[arg(long, default_value = "/ip4/127.0.0.1/tcp/0")]
        listen: String,

        /// Modo demo sin red: solo valida y publica localmente.
        #[arg(long)]
        demo: bool,

        /// Dirección completa del peer remoto, incluyendo /p2p/<PeerId>.
        #[arg(long = "peer", conflicts_with = "demo")]
        peers: Vec<String>,

        /// Topic GossipSub al que suscribirse. Se puede repetir.
        #[arg(long = "topic", conflicts_with = "demo")]
        topics: Vec<String>,

        /// Bloque WONB 0.2 que se publicará desde este nodo.
        #[arg(long, conflicts_with = "demo")]
        publish: Option<PathBuf>,

        /// Emisores Ed25519 confiables para verificar WONB 0.2.
        #[arg(long, conflicts_with = "demo")]
        trusted_issuers: Option<PathBuf>,

        /// Archivo privado de identidad libp2p para conservar el PeerId del nodo.
        #[arg(long, conflicts_with = "demo")]
        identity_key: Option<PathBuf>,

        /// Timeout de arranque, conexión y espera de suscripción (segundos).
        #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u64).range(1..))]
        timeout_secs: u64,
    },
    /// Publica un WONB desde un archivo JSON.
    Publish {
        /// Ruta al archivo JSON del WONB.
        #[arg(long)]
        file: PathBuf,
    },
    /// Valida un archivo WONB sin publicarlo.
    Validate {
        /// Ruta al archivo JSON del WONB.
        #[arg(long)]
        file: PathBuf,
    },
}

/// Argumentos de línea de comandos del nodo WOMBAT.
#[derive(Debug, Parser)]
#[command(name = "wombat-node")]
#[command(about = "Nodo P2P del protocolo WOMBAT (WONB)")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// Configuración cargada del nodo.
#[cfg_attr(not(feature = "p2p"), allow(dead_code))]
#[derive(Debug, Clone)]
pub struct NodeConfig {
    pub listen_addr: String,
    pub demo: bool,
    pub peers: Vec<String>,
    pub topics: Vec<String>,
    pub publish: Option<PathBuf>,
    pub trusted_issuers: Option<PathBuf>,
    pub identity_key: Option<PathBuf>,
    pub timeout_secs: u64,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            listen_addr: "/ip4/127.0.0.1/tcp/0".to_string(),
            demo: false,
            peers: Vec::new(),
            topics: Vec::new(),
            publish: None,
            trusted_issuers: None,
            identity_key: None,
            timeout_secs: 30,
        }
    }
}
