use crate::config::{Config, Mode};
use regex::Regex;
use reqwest::{Client, Url};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{debug, info};

pub struct AppState {
    pub client: Client,
    pub backends: Vec<(Regex, Url, Option<String>)>,
    pub semaphore: Arc<Semaphore>,
    pub mode: Mode,
    pub max_request_body_bytes: usize,
}

impl AppState {
    pub fn from_config(cfg: &Config) -> anyhow::Result<Self> {
        let timeout = std::time::Duration::from_secs(cfg.timeout_secs.unwrap_or(5));
        let mut client_builder = Client::builder().timeout(timeout);

        // Apply connect timeout if configured
        if let Some(connect_timeout_secs) = cfg.connect_timeout_secs {
            let connect_timeout = std::time::Duration::from_secs(connect_timeout_secs);
            client_builder = client_builder.connect_timeout(connect_timeout);
            debug!("Connect timeout set to: {:?}", connect_timeout);
        }

        // Apply pool max idle per host if configured
        if let Some(pool_max_idle) = cfg.pool_max_idle_per_host {
            client_builder = client_builder.pool_max_idle_per_host(pool_max_idle);
            debug!("Pool max idle per host set to: {}", pool_max_idle);
        }

        // Apply TCP keepalive if configured
        if let Some(tcp_keepalive_secs) = cfg.tcp_keepalive_secs {
            let tcp_keepalive = std::time::Duration::from_secs(tcp_keepalive_secs);
            client_builder = client_builder.tcp_keepalive(Some(tcp_keepalive));
            debug!("TCP keepalive set to: {:?}", tcp_keepalive);
        }

        let client = client_builder.build()?;
        debug!("HTTP client created with timeout: {:?}", timeout);

        let mut backends = Vec::new();
        for b in &cfg.backends {
            let re = Regex::new(&b.pattern).map_err(|e| anyhow::anyhow!(e))?;
            // Parse and validate backend URL at startup
            let url = Url::parse(&b.url)
                .map_err(|e| anyhow::anyhow!("Invalid backend URL '{}': {}", b.url, e))?;
            backends.push((re, url, b.token.clone()));
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
            connect_timeout_secs: None,
            pool_max_idle_per_host: None,
            tcp_keepalive_secs: None,
        };
        let st = AppState::from_config(&cfg).expect("build state");
        // mode should be set to Simple
        match st.mode {
            Mode::Simple => {}
            _ => panic!("expected Simple mode"),
        }
        assert_eq!(st.backends.len(), 1, "should have one backend compiled");
    }

    #[test]
    fn appstate_rejects_invalid_backend_url() {
        let cfg = Config {
            listen: None,
            backends: vec![Backend {
                pattern: "^test".to_string(),
                url: "not-a-valid-url".to_string(),
                token: None,
            }],
            timeout_secs: Some(1),
            max_outbound_concurrency: Some(4),
            mode: Some(Mode::Multi),
            max_request_body_bytes: None,
            connect_timeout_secs: None,
            pool_max_idle_per_host: None,
            tcp_keepalive_secs: None,
        };
        let result = AppState::from_config(&cfg);
        assert!(result.is_err(), "should fail with invalid URL");
        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(
                err_msg.contains("Invalid backend URL"),
                "error message should mention invalid URL: {}",
                err_msg
            );
        }
    }

    #[test]
    fn appstate_builds_client_with_tuning_options() {
        let cfg = Config {
            listen: None,
            backends: vec![Backend {
                pattern: "^test".to_string(),
                url: "http://127.0.0.1:8080".to_string(),
                token: None,
            }],
            timeout_secs: Some(10),
            max_outbound_concurrency: Some(16),
            mode: Some(Mode::Multi),
            max_request_body_bytes: None,
            connect_timeout_secs: Some(2),
            pool_max_idle_per_host: Some(32),
            tcp_keepalive_secs: Some(60),
        };
        // Should build successfully with all tuning options
        let st = AppState::from_config(&cfg).expect("build state with tuning options");
        assert_eq!(st.backends.len(), 1, "should have one backend");
        // The client is created, we can't directly inspect its internal settings
        // but if it builds without error, the configuration was accepted
    }

    #[test]
    fn appstate_works_without_tuning_options() {
        let cfg = Config {
            listen: None,
            backends: vec![Backend {
                pattern: "^test".to_string(),
                url: "http://127.0.0.1:8080".to_string(),
                token: None,
            }],
            timeout_secs: Some(5),
            max_outbound_concurrency: Some(8),
            mode: None,
            max_request_body_bytes: None,
            connect_timeout_secs: None,
            pool_max_idle_per_host: None,
            tcp_keepalive_secs: None,
        };
        // Should work with defaults (backward compatibility)
        let st = AppState::from_config(&cfg).expect("build state without tuning options");
        assert_eq!(st.backends.len(), 1, "should have one backend");
    }
}
