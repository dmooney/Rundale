# ── Stage 1: build Svelte frontend ───────────────────────────────────────────
FROM node:22-slim AS frontend
WORKDIR /build
COPY ui/package*.json ./
RUN npm ci
COPY ui/ ./
RUN npm run build

# ── Stage 2: build Rust binary ────────────────────────────────────────────────
FROM rust:1.86-slim-bookworm AS builder
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY crates/ crates/
COPY src-tauri/ src-tauri/
# Build only the parish binary (excludes the Tauri desktop binary)
RUN cargo build --release --bin parish

# ── Stage 3: minimal runtime image ────────────────────────────────────────────
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /build/target/release/parish ./
COPY --from=frontend /build/dist ./ui/dist/
COPY data/ ./data/
COPY mods/ ./mods/

ENV RUST_LOG=info
EXPOSE 3001
# Railway injects $PORT; fall back to 3001 for local docker testing
CMD sh -c "./parish --web ${PORT:-3001}"
