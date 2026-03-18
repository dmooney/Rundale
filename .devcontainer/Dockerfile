FROM mcr.microsoft.com/devcontainers/rust:1-bookworm

# Install additional tools
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl \
    sqlite3 \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Install just
RUN curl --proto '=https' --tlsv1.2 -sSf https://just.systems/install.sh | bash -s -- --to /usr/local/bin

# Install useful cargo tools
RUN cargo install cargo-tarpaulin cargo-watch cargo-outdated cargo-audit

# Set default environment
ENV RUST_BACKTRACE=1
ENV RUST_LOG=info
