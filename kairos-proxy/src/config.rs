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
