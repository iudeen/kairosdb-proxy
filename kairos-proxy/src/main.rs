mod config;
mod proxy;
mod query_metric;
mod query_metric_tags;
mod state;

use axum::Router;
use config::Config;
use state::AppState;
use std::{net::SocketAddr, sync::Arc};
use tokio::signal;
use tracing::{debug, info, warn};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Configure logging with proper defaults for container environments
    // LOG_LEVEL env var controls the log level (default: info)
    // Supports: error, warn, info, debug, trace
    let log_level = std::env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&log_level));
    
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .init();

    let config_path = std::env::var("KAIROS_PROXY_CONFIG").unwrap_or_else(|_| "config.toml".into());
    info!("Loading configuration from: {}", config_path);
    let cfg = Config::from_file(&config_path)?;
    debug!("Configuration loaded successfully with {} backend(s)", cfg.backends.len());

    let state = Arc::new(AppState::from_config(&cfg)?);
    info!(
        "Proxy configured with mode: {:?}, max_outbound_concurrency: {}, timeout: {}s",
        state.mode,
        cfg.max_outbound_concurrency.unwrap_or(32),
        cfg.timeout_secs.unwrap_or(5)
    );

    let app = Router::new()
        .route("/health", axum::routing::get(proxy::health_handler))
        .route(
            "/api/v1/datapoints/query/tags",
            axum::routing::post(proxy::query_metric_tags_handler),
        )
        .route(
            "/api/v1/datapoints/query",
            axum::routing::post(proxy::query_metric_handler),
        )
        .with_state(state);

    let listen = cfg.listen.unwrap_or_else(|| "0.0.0.0:8080".into());
    let addr: SocketAddr = listen.parse()?;
    info!("Starting kairos-proxy server on {}", addr);
    info!("Available endpoints: /health, /api/v1/datapoints/query, /api/v1/datapoints/query/tags");

    let server = axum::Server::bind(&addr).serve(app.into_make_service());

    let graceful = server.with_graceful_shutdown(shutdown_signal());
    graceful.await?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = signal::ctrl_c().await;
    warn!("Shutdown signal received, gracefully terminating...");
}
