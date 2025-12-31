use serde::Deserialize;
use std::fs;

#[derive(Debug, Deserialize)]
pub struct Backend {
    pub pattern: String,
    pub url: String,
    pub token: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Simple,
    #[default]
    Multi,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub listen: Option<String>,
    pub backends: Vec<Backend>,
    pub timeout_secs: Option<u64>,
    // Maximum number of concurrent outbound requests across all handlers
    // If not set, a sensible default will be used in `AppState`.
    pub max_outbound_concurrency: Option<usize>,
    // Operation mode: `simple` for single-metric forwarding, `multi` to split by metric and merge
    // Defaults to `multi`.
    pub mode: Option<Mode>,
    // Maximum request body size in bytes. Requests exceeding this will return 413 Payload Too Large.
    // If not set, defaults to 5 MB (5_242_880 bytes).
    pub max_request_body_bytes: Option<usize>,
    // Connection timeout in seconds for establishing connections to backends.
    // If not set, uses reqwest's default behavior (no specific connect timeout).
    pub connect_timeout_secs: Option<u64>,
    // Maximum number of idle connections to keep alive per host.
    // If not set, uses reqwest's default (no specific limit).
    pub pool_max_idle_per_host: Option<usize>,
    // TCP keepalive interval in seconds to detect dead connections.
    // If not set, uses system default TCP keepalive settings.
    pub tcp_keepalive_secs: Option<u64>,
}

impl Config {
    pub fn from_file(path: &str) -> anyhow::Result<Self> {
        let cfg_str = fs::read_to_string(path)?;
        Ok(toml::from_str(&cfg_str)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_default_is_multi() {
        let m = Mode::default();
        match m {
            Mode::Multi => {}
            _ => panic!("Mode default should be Multi"),
        }
    }

    #[test]
    fn parse_example_config() {
        let s = fs::read_to_string("config.toml.example").expect("read example config");
        let cfg: Config = toml::from_str(&s).expect("parse example toml");
        assert!(
            !cfg.backends.is_empty(),
            "example config should define backends"
        );
    }
}
