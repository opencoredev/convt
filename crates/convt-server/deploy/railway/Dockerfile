# syntax=docker/dockerfile:1
# Repository root is the build context. Intended final path: crates/convt-server/deploy/railway/Dockerfile.
FROM rust:1.95.0-bookworm AS build
WORKDIR /build
ENV SQLX_OFFLINE=true
COPY Cargo.toml Cargo.lock ./
COPY vendor/ vendor/
# Retain all workspace manifests and the crate-local SQLx metadata.
COPY crates/ crates/
# build.rs hashes these files; it does not connect to Postgres or run migrations.
COPY packages/db/migrations/ packages/db/migrations/
RUN cargo build --locked --release -p convt-server

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libgcc-s1 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /build/target/release/convt-server /usr/local/bin/convt-server
# PORT is injected at runtime by Railway; the application reads it directly.
ENV CONVT_BIND_HOST=0.0.0.0
USER 10001:10001
WORKDIR /tmp
ENTRYPOINT ["/usr/local/bin/convt-server"]
