FROM rust:1.71-slim as builder
WORKDIR /usr/src/kairos-proxy
COPY . .
RUN cargo build --release --manifest-path kairos-proxy/Cargo.toml

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=builder /usr/src/kairos-proxy/target/release/kairos-proxy /usr/local/bin/kairos-proxy
COPY kairos-proxy/config.toml.example /app/config.toml
EXPOSE 8080
USER 65532:65532
ENTRYPOINT ["/usr/local/bin/kairos-proxy"]
