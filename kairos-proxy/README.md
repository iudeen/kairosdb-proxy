# kairos-proxy (crate)

This folder contains the `kairos-proxy` binary crate — the lightweight Rust proxy that routes KairosDB REST calls to different backends based on metric name.

Quick commands (from repo root):

```bash
# Build the binary
cargo build --manifest-path kairos-proxy/Cargo.toml --release

# Run locally with example config
KAIROS_PROXY_CONFIG=./kairos-proxy/config.toml.example ./target/release/kairos-proxy

# Run tests for this crate
cargo test --manifest-path kairos-proxy/Cargo.toml
```

Notes
- Crate-level documentation and examples live here. See the top-level `README.md` for higher-level project notes.
- Config lives in `config.toml.example` (copy to `config.toml` or set `KAIROS_PROXY_CONFIG`).
