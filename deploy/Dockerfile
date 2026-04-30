# Build context: repository root.
# Build:  docker build -f deploy/Dockerfile -t parish .
#
# ── Stage 1: build Svelte frontend ───────────────────────────────────────────
FROM node:22-slim AS frontend
WORKDIR /build
COPY parish/apps/ui/package*.json ./
RUN npm ci
COPY parish/apps/ui/ ./
RUN npm run build

# ── Stage 2: build Rust binary ────────────────────────────────────────────────
FROM rust:1.95.0-slim-bookworm AS builder
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
# Mirror the repo layout so source-relative include_str! paths
# (parish/crates/parish-server/src/lib.rs → ../../../../LICENSE) resolve.
COPY parish/Cargo.toml parish/Cargo.lock parish/
COPY parish/crates/ parish/crates/
# Compile-time embedding targets (see parish-server/src/lib.rs)
COPY LICENSE NOTICE THIRD_PARTY_NOTICES.md ./
# Build only the parish binary (excludes the Tauri desktop binary)
WORKDIR /build/parish
RUN cargo build --release -p parish --bin parish

# ── Stage 3: minimal runtime image ────────────────────────────────────────────
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Download cloudflared at a pinned version and verify against an inline SHA-256.
# Cloudflare does not publish per-asset .sha256 files, so we pin the digests here
# and verify locally. Bump CLOUDFLARED_VERSION and both SHA256 constants together.
# TARGETARCH is set automatically by BuildKit (amd64, arm64).
# Both amd64 and arm64 hashes are pinned inline; the correct one is selected via
# a shell case statement so multi-arch builds are integrity-checked on every arch.
ARG TARGETARCH
ARG CLOUDFLARED_VERSION=2026.3.0
RUN cd /tmp \
 && curl -fsSL \
      "https://github.com/cloudflare/cloudflared/releases/download/${CLOUDFLARED_VERSION}/cloudflared-linux-${TARGETARCH}" \
      -o cloudflared \
 && case "${TARGETARCH}" in \
      amd64) SHA256="4a9e50e6d6d798e90fcd01933151a90bf7edd99a0a55c28ad18f2e16263a5c30" ;; \
      arm64) SHA256="0755ba4cbab59980e6148367fcf53a8f3ec85a97deefd63c2420cf7850769bee" ;; \
      *) echo "Unsupported TARGETARCH: ${TARGETARCH}" && exit 1 ;; \
    esac \
 && echo "${SHA256}  cloudflared" | sha256sum -c - \
 && install -m 0755 cloudflared /usr/local/bin/cloudflared \
 && rm cloudflared

WORKDIR /app
COPY --from=builder /build/parish/target/release/parish ./
COPY --from=frontend /build/dist ./apps/ui/dist/
COPY mods/ ./mods/
RUN useradd -m -u 1000 app && chown -R app:app /app
USER app

ENV RUST_LOG=info
EXPOSE 3001
# Railway injects $PORT; fall back to 3001 for local docker testing
CMD sh -c "./parish --web ${PORT:-3001}"
