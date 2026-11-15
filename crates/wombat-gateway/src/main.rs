use std::net::SocketAddr;

use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::EnvFilter;
use wombat_gateway::{allowed_origin_from_env, bind_address_from_env, router, RiskStore};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let address: SocketAddr = bind_address_from_env()?;
    let allowed_origin = allowed_origin_from_env()?;
    let listener = TcpListener::bind(address).await?;
    info!(%address, "WOMBAT gateway HTTP escuchando");

    axum::serve(listener, router(RiskStore::default(), allowed_origin)).await?;
    Ok(())
}
