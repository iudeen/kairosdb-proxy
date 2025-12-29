use crate::config::{Config, Mode};
use regex::Regex;
use reqwest::Client;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{debug, info};

pub struct AppState {
    pub client: Client,
    pub backends: Vec<(Regex, String, Option<String>)>,
    pub semaphore: Arc<Semaphore>,
    pub mode: Mode,
    pub max_request_body_bytes: usize,
}

impl AppState {
    pub fn from_config(cfg: &Config) -> anyhow::Result<Self> {
        let timeout = std::time::Duration::from_secs(cfg.timeout_secs.unwrap_or(5));
        let client = Client::builder().timeout(timeout).build()?;
        debug!("HTTP client created with timeout: {:?}", timeout);

        let mut backends = Vec::new();
        for b in &cfg.backends {
            let re = Regex::new(&b.pattern).map_err(|e| anyhow::anyhow!(e))?;
            backends.push((re, b.url.clone(), b.token.clone()));
            info!(
                "Registered backend: pattern='{}' -> url='{}'",
                b.pattern, b.url
            );
        }

        let max_outbound = cfg.max_outbound_concurrency.unwrap_or(32);
        let semaphore = Arc::new(Semaphore::new(max_outbound));
        debug!("Created semaphore with {} permits", max_outbound);

        let mode = cfg.mode.clone().unwrap_or_default();

        // Default to 5 MB if not specified
        const DEFAULT_MAX_BODY_BYTES: usize = 5_242_880; // 5 MB
        const BYTES_PER_MB: usize = 1_048_576;
        let max_request_body_bytes = cfg.max_request_body_bytes.unwrap_or(DEFAULT_MAX_BODY_BYTES);
        debug!(
            "Maximum request body size: {} bytes ({} MB)",
            max_request_body_bytes,
            max_request_body_bytes / BYTES_PER_MB
        );

        Ok(AppState {
            client,
            backends,
            semaphore,
            mode,
            max_request_body_bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Backend;

    #[test]
    fn appstate_from_config_sets_mode_and_backends() {
        let cfg = Config {
            listen: None,
            backends: vec![Backend {
                pattern: "^a".to_string(),
                url: "http://127.0.0.1:9000".to_string(),
                token: None,
            }],
            timeout_secs: Some(1),
            max_outbound_concurrency: Some(4),
            mode: Some(Mode::Simple),
            max_request_body_bytes: None,
        };
        let st = AppState::from_config(&cfg).expect("build state");
        // mode should be set to Simple
        match st.mode {
            Mode::Simple => {}
            _ => panic!("expected Simple mode"),
        }
        assert_eq!(st.backends.len(), 1, "should have one backend compiled");
    }
}
