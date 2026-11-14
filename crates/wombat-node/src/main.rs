//! Punto de entrada del nodo WOMBAT.
//!
//! Comandos:
//!   wombat-node run                -> inicia el nodo P2P (modo demo por defecto)
//!   wombat-node publish --file ... -> publica un WONB desde JSON
//!   wombat-node validate --file ...-> valida un WONB sin publicarlo

use anyhow::Result;
use clap::Parser;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use wombat_core::{validate_wonb, Wonb};

use crate::config::{Cli, Commands, NodeConfig};

mod behaviour;
mod config;

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
}

async fn run_cmd(config: NodeConfig) -> Result<()> {
    info!("Iniciando nodo WOMBAT (demo={})", config.demo);
    if config.demo {
        info!("Modo demo local: sin red P2P. Usa publish/validate para probar.");
        return Ok(());
    }
    info!(
        "Modo demo activo: sin red P2P. Topics configurados: {:?}",
        config.topics
    );
    info!(
        "Para conectar peers reales, habilite la característica `p2p` con libp2p \
         y configure Multiaddrs. Ver docs/demo-runbook.md."
    );
    Ok(())
}

async fn publish_cmd(file: std::path::PathBuf) -> Result<()> {
    let content = std::fs::read_to_string(&file)
        .map_err(|e| anyhow::anyhow!("no se pudo leer {}: {}", file.display(), e))?;
    let wonb: Wonb =
        serde_json::from_str(&content).map_err(|e| anyhow::anyhow!("JSON inválido: {e}"))?;
    validate_wonb(&wonb).map_err(|e| anyhow::anyhow!("WONB inválido: {e}"))?;
    info!(
        "WONB válido listo para publicar: id={} tipo={} topic={}",
        wonb.id,
        wonb.type_,
        wonb.topic()
    );
    println!("{}", serde_json::to_string_pretty(&wonb)?);
    Ok(())
}

async fn validate_cmd(file: std::path::PathBuf) -> Result<()> {
    let content = std::fs::read_to_string(&file)
        .map_err(|e| anyhow::anyhow!("no se pudo leer {}: {}", file.display(), e))?;
    let wonb: Wonb =
        serde_json::from_str(&content).map_err(|e| anyhow::anyhow!("JSON inválido: {e}"))?;
    validate_wonb(&wonb).map_err(|e| anyhow::anyhow!("WONB inválido: {e}"))?;
    info!(
        "WONB válido: id={} tipo={} topic={}",
        wonb.id,
        wonb.type_,
        wonb.topic()
    );
    println!("WONB válido: {}", wonb.id);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let cli = Cli::parse();

    match cli.command {
        Commands::Run { listen, demo } => {
            let config = NodeConfig {
                listen_addr: listen,
                demo,
                ..NodeConfig::default()
            };
            run_cmd(config).await
        }
        Commands::Publish { file } => publish_cmd(file).await,
        Commands::Validate { file } => validate_cmd(file).await,
    }
    .map_err(|e| {
        error!("{e}");
        e
    })
}

#[cfg(test)]
mod tests {
    //use super::*;
    use crate::config::{DEFAULT_TOPICS, TOPIC_PREFIX};

    #[test]
    fn default_topics_are_wombat_prefixed() {
        for t in DEFAULT_TOPICS {
            assert!(t.starts_with(TOPIC_PREFIX));
        }
    }
}
