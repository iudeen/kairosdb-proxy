# Copilot instructions

## Project overview

- This repository is a Rust workspace; the `kairos-proxy` package is the HTTP proxy binary.
- The application uses Axum and Reqwest. Its handlers proxy KairosDB query and tag-query requests to configured backends, with routing and response handling implemented in `kairos-proxy/src/`.
- Backend configuration is TOML, and `KAIROS_PROXY_CONFIG` selects the config file at runtime.
- The `tests/` directory contains Python integration tests and mock KairosDB servers. See `tests/README.md` for setup details.

## Working in this repository

- Keep changes focused on the requested behavior and follow the existing Rust style and module structure.
- Preserve request and response behavior across both proxy modes (`simple` and `multi`). Add or update tests when changing routing, query handling, configuration, or HTTP behavior.
- Keep configuration examples and documentation in sync when changing configuration options.
- Avoid adding dependencies unless existing crates cannot reasonably support the change.

## Validation

Run these commands from the repository root for Rust changes:

```bash
cargo fmt --all -- --check
cargo clippy --all -- -D warnings
cargo test --manifest-path kairos-proxy/Cargo.toml
```

For changes affecting end-to-end HTTP behavior, run the integration suite as described in `tests/README.md`. It requires the Python dependencies in `tests/requirements.txt`, a release build of the proxy, and the mock servers.
