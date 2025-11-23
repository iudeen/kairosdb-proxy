mod config;
mod proxy;
mod state;
mod query_metric;
mod query_metric_tags;

use axum::Router;
use config::Config;
use state::AppState;
use std::{net::SocketAddr, sync::Arc};
use tokio::signal;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config_path = std::env::var("KAIROS_PROXY_CONFIG").unwrap_or_else(|_| "config.toml".into());
    let cfg = Config::from_file(&config_path)?;

    let state = Arc::new(AppState::from_config(&cfg)?);

    let app = Router::new()
        .route("/api/v1/datapoints/query/tags", axum::routing::post(proxy::query_metric_tags_handler))
        .route("/api/v1/datapoints/query", axum::routing::post(proxy::query_metric_handler))
        .with_state(state);

    let listen = cfg.listen.unwrap_or_else(|| "0.0.0.0:8080".into());
    let addr: SocketAddr = listen.parse()?;
    info!(%addr, "Starting kairos-proxy");

    let server = axum::Server::bind(&addr).serve(app.into_make_service());

    let graceful = server.with_graceful_shutdown(shutdown_signal());
    graceful.await?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = signal::ctrl_c().await;
    info!("Shutdown signal received");
}

