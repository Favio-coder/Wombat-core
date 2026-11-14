//! Configuración del nodo WOMBAT.
//!
//! Define los topics por defecto a los que se suscribe el nodo y los
//! parámetros de arranque del transporte libp2p.

use clap::Parser;
use std::path::PathBuf;

/// Topics por defecto del MVP WOMBAT.
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
    /// Ejecuta el nodo P2P (modo demo local por defecto).
    Run {
        /// Escuchar en esta dirección multiaddr (ej. /ip4/0.0.0.0/tcp/0).
        #[arg(long, default_value = "/ip4/0.0.0.0/tcp/0")]
        listen: String,

        /// Modo demo sin red: solo valida y publica localmente.
        #[arg(long)]
        demo: bool,
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
#[derive(Debug, Clone)]
pub struct NodeConfig {
    #[allow(dead_code)]
    pub listen_addr: String,
    pub demo: bool,
    pub topics: Vec<String>,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            listen_addr: "/ip4/0.0.0.0/tcp/0".to_string(),
            demo: false,
            topics: DEFAULT_TOPICS.iter().map(|t| (*t).to_string()).collect(),
        }
    }
}
