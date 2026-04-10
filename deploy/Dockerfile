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
    ca-certificates curl libssl3 bash jq \
    && rm -rf /var/lib/apt/lists/*

# Download cloudflared at a pinned version and verify its SHA-256 checksum
# before installing.  Never fetch an unpinned "latest" binary from the internet.
# The upstream .sha256 file is tied to the original asset filename, so we must
# verify under that filename and only then install to /usr/local/bin.
ARG CLOUDFLARED_VERSION=2024.12.2
RUN cd /tmp \
 && curl -fsSL \
      "https://github.com/cloudflare/cloudflared/releases/download/${CLOUDFLARED_VERSION}/cloudflared-linux-amd64" \
      -o cloudflared-linux-amd64 \
 && curl -fsSL \
      "https://github.com/cloudflare/cloudflared/releases/download/${CLOUDFLARED_VERSION}/cloudflared-linux-amd64.sha256" \
      -o cloudflared-linux-amd64.sha256 \
 && sha256sum -c cloudflared-linux-amd64.sha256 \
 && install -m 0755 cloudflared-linux-amd64 /usr/local/bin/cloudflared \
 && rm cloudflared-linux-amd64 cloudflared-linux-amd64.sha256

WORKDIR /app
COPY --from=builder /build/target/release/parish ./
COPY --from=frontend /build/dist ./apps/ui/dist/
COPY mods/ ./mods/
COPY deploy/entrypoint.sh ./entrypoint.sh
RUN chmod +x ./entrypoint.sh
RUN useradd -m -u 1000 app && chown -R app:app /app
USER app

ENV RUST_LOG=info
EXPOSE 3001
# entrypoint.sh detects PR vs production via RAILWAY_ENVIRONMENT and
# starts cloudflared tunnel (PR only) alongside the parish binary.
CMD ["./entrypoint.sh"]
