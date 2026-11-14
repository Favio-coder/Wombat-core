//! Punto de entrada del nodo WOMBAT.
//!
//! Comandos:
//!   wombat-node run --demo         -> modo local sin red
//!   cargo run -p wombat-node --features p2p -- run -> P2P TCP/GossipSub
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
#[cfg(feature = "p2p")]
mod p2p;

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
    #[cfg(feature = "p2p")]
    {
        p2p::run(config).await
    }
    #[cfg(not(feature = "p2p"))]
    {
        let _ = config;
        anyhow::bail!("modo P2P no disponible: recompila con `--features p2p`")
    }
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
        Commands::Run {
            listen,
            demo,
            peers,
            topics,
            publish,
            trusted_issuers,
            timeout_secs,
        } => {
            let config = NodeConfig {
                listen_addr: listen,
                demo,
                peers,
                topics,
                publish,
                trusted_issuers,
                timeout_secs,
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
    use std::path::PathBuf;

    use super::*;
    use crate::config::{DEFAULT_TOPICS, TOPIC_PREFIX};

    #[test]
    fn default_topics_are_wombat_prefixed() {
        for t in DEFAULT_TOPICS {
            assert!(t.starts_with(TOPIC_PREFIX));
        }
    }

    #[tokio::test]
    async fn demo_returns_without_starting_p2p() {
        run_cmd(NodeConfig {
            demo: true,
            ..NodeConfig::default()
        })
        .await
        .unwrap();
    }

    #[cfg(not(feature = "p2p"))]
    #[tokio::test]
    async fn real_mode_without_feature_returns_clear_error() {
        let error = run_cmd(NodeConfig::default()).await.unwrap_err();
        assert!(error.to_string().contains("--features p2p"));
    }

    #[tokio::test]
    async fn isolated_publish_command_is_only_local_validation() {
        let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../contracts/fixtures/anomaly-valid.json");
        publish_cmd(file).await.unwrap();
    }
}
