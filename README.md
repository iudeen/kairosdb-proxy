# Kairos Proxy

Lightweight Rust proxy that routes REST API requests to different KairosDB instances based on metric names.


Quick start

1. Build locally:

```bash
cd /Users/iudeen/KairosProxy
cargo build --manifest-path kairos-proxy/Cargo.toml --release
```

2. Run with example config:

```bash
KAIROS_PROXY_CONFIG=./kairos-proxy/config.toml.example ./target/release/kairos-proxy
```

3. Docker build & run:

```bash
docker compose up --build
```


Metric Routing Logic

- The proxy determines the metric name for backend routing as follows:
	1. Checks for the `X-METRICNAME` HTTP header (case-insensitive).
	2. If not found, parses the JSON body for:
		 - `metrics[0].name` (as in KairosDB query API)
		 - or `metric`
		 - or `metricName`
- Query parameters and trailing path segments are **not** used for metric extraction.

Configuration

- `KAIROS_PROXY_CONFIG` environment variable points to a TOML file with `backends` entries. See `kairos-proxy/config.toml.example`.

Notes

- This is a minimal starter. In production, add request size limits, TLS configuration, stronger auth, and metrics.
