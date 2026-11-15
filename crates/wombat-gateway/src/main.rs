use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::EnvFilter;
use wombat_gateway::{
    allowed_origin_from_env, follow_event_file, router, EventStore, GatewayArgs, RiskStore,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = <GatewayArgs as clap::Parser>::parse();
    let allowed_origin = allowed_origin_from_env()?;
    let events = EventStore::default();
    if let Some(path) = args.event_in {
        info!(path = %path.display(), "Siguiendo eventos WONB verificados");
        tokio::spawn(follow_event_file(path, events.clone()));
    } else {
        info!("WOMBAT_GATEWAY_EVENT_IN/--event-in no configurado; API sin eventos JSONL");
    }

    let listener = TcpListener::bind(args.bind).await?;
    info!(address = %args.bind, "WOMBAT gateway HTTP escuchando");

    axum::serve(
        listener,
        router(RiskStore::default(), events, allowed_origin),
    )
    .await?;
    Ok(())
}
