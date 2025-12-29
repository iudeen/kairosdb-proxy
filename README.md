# Kairos Proxy — fast, tiny, and metric-aware

This project is a Rust proxy that routes KairosDB-style REST requests to different KairosDB backends based on metric names. It is designed for speed, low memory footprint, and simple container deployment. If you like minimal surface area and fast I/O, you and this proxy will get along.

**Quick highlights**
- Written in Rust using `axum` + `reqwest` (async, hyper-based).
- Routes `/api/v1/datapoints/query` and `/api/v1/datapoints/query/tags` by metric name.
- Two modes: `simple` (fast streaming pass-through) and `multi` (split-and-merge for multi-metric queries).
- Bounded outbound concurrency (configurable) to protect backends and the proxy.
- Small, container-friendly Dockerfile with a lightweight `HEALTHCHECK`.

**Who is this README for?**
- Developers integrating KairosDB clusters behind a single endpoint.
- SREs wanting a thin, configurable routing layer for multi-backend setups.
- Rust engineers who want to extend or harden the proxy.

**Why this proxy exists**

KairosDB is a great TSDB, but it doesn't provide a built-in way to route requests across multiple independent KairosDB backends based on metric name. In real deployments teams often split metrics across clusters for scale, retention, or organizational boundaries (example: `hot` cluster for high-cardinality telemetry, `warm` cluster for medium-term retention, and an `archive` cluster). KairosDB itself expects clients to know where to send a metric.

This proxy fills that gap: it lets you present a single public endpoint and route requests to the appropriate backend using lightweight regex rules. It keeps the routing logic thin and observable, and, when configured in `Simple` mode, streams responses unchanged for minimal overhead.

**Quick start**

- Using Docker (recommended):

```bash
# Pull the latest release from GitHub Container Registry
docker pull ghcr.io/iudeen/kairosdb-proxy:latest

# Run with your config
docker run -p 8080:8080 \
  -v $(pwd)/config.toml:/app/config.toml \
  -e KAIROS_PROXY_CONFIG=/app/config.toml \
  ghcr.io/iudeen/kairosdb-proxy:latest
```

- Build locally (from repo root):

```bash
cargo build --manifest-path kairos-proxy/Cargo.toml --release
```

- Run using the example config:

```bash
KAIROS_PROXY_CONFIG=./kairos-proxy/config.toml.example ./target/release/kairos-proxy
```

- Run in Docker Compose (development):

```bash
docker compose up --build
```

**Configuration**

- Default config file: `kairos-proxy/config.toml.example`.
- Path can be overridden with the env var `KAIROS_PROXY_CONFIG`.
- Important config fields:
	- `listen`: host:port for the proxy (default: `0.0.0.0:8080`).
	- `backends`: ordered list of `{ pattern = "<regex>", url = "http://...", token = "..." }` mapping metric name regex → backend URL (token optional).
	- `timeout_secs`: per-backend request timeout.
	- `max_outbound_concurrency`: cap on concurrent requests the proxy will make to backends.
	- `mode`: `Simple` (default streaming pass-through) or `Multi` (split multi-metric requests and merge responses).

**Logging**

The proxy uses structured logging with configurable log levels for better observability, especially in container environments.

- Set the `LOG_LEVEL` environment variable to control logging verbosity (default: `info`)
- Supported log levels: `error`, `warn`, `info`, `debug`, `trace`
- Alternatively, use `RUST_LOG` for fine-grained control (e.g., `RUST_LOG=kairos_proxy=debug`)

Example with different log levels:
```bash
# Minimal logging (errors and warnings only)
LOG_LEVEL=warn ./target/release/kairos-proxy

# Standard logging (recommended for production)
LOG_LEVEL=info ./target/release/kairos-proxy

# Detailed logging (useful for debugging)
LOG_LEVEL=debug ./target/release/kairos-proxy
```

In Docker/Docker Compose:
```yaml
environment:
  - LOG_LEVEL=info
```

Example snippet (see `config.toml.example`):

```toml
[[backends]]
pattern = "^cpu\\..*"
url = "http://kairos-cpu:8080"

[[backends]]
pattern = "^mem\\..*"
url = "http://kairos-mem:8080"

timeout_secs = 5
max_outbound_concurrency = 8
mode = "Multi"
```

**Routing & modes**

- Metric extraction (how the proxy decides where to send a request):
	- If present: `X-METRICNAME` HTTP header (case-insensitive) takes precedence.
	- Otherwise: parse JSON body — look for `metrics[0].name` (Kairos query), or `metric` / `metricName` fields.
	- If no metric can be determined, the proxy returns `502 Bad Gateway`.

- Modes:
	- `Simple`: Fast path — the proxy picks the backend based on the *first* metric name but forwards the *entire* original request payload unchanged. Responses are streamed from the backend directly to the client (low memory, low latency).
	- `Multi`: The proxy groups metrics by backend, sends one request per backend containing only its relevant metrics, waits for JSON responses, and merges the results into a single KairosDB-style response. This requires buffering the JSON from backends so merging can happen.

Choose `Simple` for throughput/low-footprint scenarios where the first metric reliably identifies the correct backend. Choose `Multi` when you must merge results from multiple backends for multi-metric requests.

**Performance & safety knobs**

- `max_outbound_concurrency` — prevents the proxy from flooding backends. Tune to backend capacity.
- `timeout_secs` — guards against slow backends; default is short to keep the proxy responsive.
- The proxy uses a `Semaphore` to bound concurrent outbound requests and `FuturesUnordered` for efficient parallelism.

Tips:
- Use `Simple` mode + streaming to keep memory usage minimal when proxying large responses.
- For better tail latency, set `max_outbound_concurrency` to a moderate number (e.g., 8–32) depending on CPU and backend throughput.

**Docker & healthcheck**

- The provided `Dockerfile` is multi-stage: build in Rust image, run a minimal Debian-based runtime image.
- The runtime image includes a small static `busybox` binary and a `HEALTHCHECK` that probes `GET /health`.

Build & run:

```bash
docker build -t kairos-proxy -f kairos-proxy/Dockerfile .
docker run --rm -p 8080:8080 -e KAIROS_PROXY_CONFIG=/app/config.toml kairos-proxy
```

**Health & metrics**

- `GET /health` returns `200` with `{"status":"ok"}` when the process is alive.

**Testing**

- Tests are self-contained and use in-process mock axum servers to validate routing and merge behavior — no real KairosDB required.

Run tests:

```bash
cd kairos-proxy
cargo test
```

Developer notes (quick architecture summary)
- `src/main.rs` — starts the axum server and wires routes.
- `src/state.rs` — builds `reqwest::Client`, compiles backend regexes, holds a `Semaphore` and `Mode`.
- `src/query_metric.rs` & `src/query_metric_tags.rs` — the two main handlers. `Simple` mode streams backend responses; `Multi` mode splits requests per backend and merges JSON results.

**Versioning and Releases**

This project follows [Semantic Versioning 2.0.0](https://semver.org/) (semver).

- **Releases** are published to [GitHub Releases](https://github.com/iudeen/kairosdb-proxy/releases)
- **Docker images** are automatically published to [GitHub Container Registry](https://github.com/iudeen/kairosdb-proxy/pkgs/container/kairosdb-proxy) on each release
- See [VERSIONING.md](VERSIONING.md) for detailed information on:
  - How to create a new release
  - Version increment guidelines (major, minor, patch)
  - Using released Docker images
  - Release process and automation

**Pull a specific version:**

```bash
docker pull ghcr.io/iudeen/kairosdb-proxy:v0.1.0
```

**Pull the latest release:**

```bash
docker pull ghcr.io/iudeen/kairosdb-proxy:latest
```

Contributing
- PRs welcome. If you add features that expand scope (e.g., additional KairosDB endpoints), include tests and update `config.toml.example`.

License & attribution
- No license file included — add one if you intend to open-source this.

Enjoy! If you want, you can:
- add Prometheus metrics and a `/metrics` endpoint,
- convert the Dockerfile to a smaller musl-based final image,
- add middleware for request size limits and CORS.
