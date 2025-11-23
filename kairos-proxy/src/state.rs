use crate::config::{Config, Mode};
use regex::Regex;
use reqwest::Client;
use std::sync::Arc;
use tokio::sync::Semaphore;

pub struct AppState {
    pub client: Client,
    pub backends: Vec<(Regex, String, Option<String>)>,
    pub semaphore: Arc<Semaphore>,
    pub mode: Mode,
}

impl AppState {
    pub fn from_config(cfg: &Config) -> anyhow::Result<Self> {
        let timeout = std::time::Duration::from_secs(cfg.timeout_secs.unwrap_or(5));
        let client = Client::builder().timeout(timeout).build()?;

        let mut backends = Vec::new();
        for b in &cfg.backends {
            let re = Regex::new(&b.pattern).map_err(|e| anyhow::anyhow!(e))?;
            backends.push((re, b.url.clone(), b.token.clone()));
        }

        let max_outbound = cfg.max_outbound_concurrency.unwrap_or(32);
        let semaphore = Arc::new(Semaphore::new(max_outbound));

        let mode = cfg.mode.clone().unwrap_or_default();

        Ok(AppState {
            client,
            backends,
            semaphore,
            mode,
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
