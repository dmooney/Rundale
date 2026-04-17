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

# Download cloudflared at a pinned version and verify its SHA-256 checksum
# before installing.  Never fetch an unpinned "latest" binary from the internet.
ARG CLOUDFLARED_VERSION=2024.12.2
RUN curl -fsSL \
      "https://github.com/cloudflare/cloudflared/releases/download/${CLOUDFLARED_VERSION}/cloudflared-linux-amd64" \
      -o /usr/local/bin/cloudflared \
 && curl -fsSL \
      "https://github.com/cloudflare/cloudflared/releases/download/${CLOUDFLARED_VERSION}/cloudflared-linux-amd64.sha256" \
      -o /tmp/cloudflared.sha256 \
 && (cd /usr/local/bin && sha256sum -c /tmp/cloudflared.sha256) \
 && chmod +x /usr/local/bin/cloudflared \
 && rm /tmp/cloudflared.sha256

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
