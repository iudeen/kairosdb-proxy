FROM rust:1.92 as builder

# Install build deps if needed (kept minimal here) and set working dir
WORKDIR /usr/src/kairos-proxy

# Optimize build cache: copy manifest first and fetch dependencies
COPY kairos-proxy/Cargo.toml kairos-proxy/Cargo.lock* ./
RUN mkdir -p kairos-proxy/src && echo "fn main() {}" > kairos-proxy/src/main.rs || true
RUN cargo build --release --manifest-path kairos-proxy/Cargo.toml || true

# Copy the full source and build the final binary
COPY . .
RUN cargo build --release --manifest-path kairos-proxy/Cargo.toml

FROM ubuntu:25.10

# Ensure TLS root certificates and minimal runtime deps are present
RUN apt-get update \
        && apt-get install -y --no-install-recommends ca-certificates busybox-static \
        && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary and default config
COPY --from=builder /usr/src/kairos-proxy/target/release/kairos-proxy /usr/local/bin/kairos-proxy
COPY kairos-proxy/config.toml.example /app/config.toml

# Create a non-root user and make sure permissions are correct
RUN groupadd --system app || true \
        && useradd --system --gid app --home /nonexistent --shell /usr/sbin/nologin app || true \
        && chown -R app:app /app /usr/local/bin/kairos-proxy

USER app
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/kairos-proxy"]

# Healthcheck uses a tiny busybox static binary to probe `/health` and keep image small
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
        CMD /bin/busybox wget -q -O - http://127.0.0.1:8080/health > /dev/null || exit 1
