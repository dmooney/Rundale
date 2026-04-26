# Build context: repository root.
# Build:  docker build -f deploy/Dockerfile -t parish .
#
# ── Stage 1: build Svelte frontend ───────────────────────────────────────────
FROM node:22-slim AS frontend
WORKDIR /build
COPY apps/ui/package*.json ./
RUN npm ci
COPY apps/ui/ ./
RUN npm run build

# ── Stage 2: build Rust binary ────────────────────────────────────────────────
FROM rust:1.88-slim-bookworm AS builder
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
# Build only the parish binary (excludes the Tauri desktop binary)
RUN cargo build --release -p parish --bin parish

# ── Stage 3: minimal runtime image ────────────────────────────────────────────
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Download cloudflared at a pinned version and verify against an inline SHA-256.
# Cloudflare does not publish per-asset .sha256 files, so we pin the digest here
# and verify locally. Bump CLOUDFLARED_VERSION and CLOUDFLARED_SHA256 together.
# TARGETARCH is set automatically by BuildKit (amd64, arm64).
# The SHA below matches the amd64 binary; for arm64 builds, override via:
#   docker build --build-arg CLOUDFLARED_SHA256=<arm64-digest> ...
ARG TARGETARCH
ARG CLOUDFLARED_VERSION=2026.3.0
ARG CLOUDFLARED_SHA256=4a9e50e6d6d798e90fcd01933151a90bf7edd99a0a55c28ad18f2e16263a5c30
RUN cd /tmp \
 && curl -fsSL \
      "https://github.com/cloudflare/cloudflared/releases/download/${CLOUDFLARED_VERSION}/cloudflared-linux-${TARGETARCH}" \
      -o cloudflared \
 && echo "${CLOUDFLARED_SHA256}  cloudflared" | sha256sum -c - \
 && install -m 0755 cloudflared /usr/local/bin/cloudflared \
 && rm cloudflared

WORKDIR /app
COPY --from=builder /build/target/release/parish ./
COPY --from=frontend /build/dist ./apps/ui/dist/
COPY mods/ ./mods/
RUN useradd -m -u 1000 app && chown -R app:app /app
USER app

ENV RUST_LOG=info
EXPOSE 3001
# Railway injects $PORT; fall back to 3001 for local docker testing
CMD sh -c "./parish --web ${PORT:-3001}"
