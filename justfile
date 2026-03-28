# Parish — An Irish Living World Text Adventure
# Run `just` or `just --list` to see all available commands.

set shell := ["bash", "-euo", "pipefail", "-c"]

# Ensure cargo and fnm are on PATH for all recipes
export PATH := env("HOME") + "/.cargo/bin:" + env("HOME") + "/.local/share/fnm:" + env("PATH")

# Default: list available commands
default:
    @just --list

# ─── Setup ───────────────────────────────────────────────────────────────────

# One-time developer setup: install Rust, Node.js, tools, and frontend dependencies
setup:
    #!/usr/bin/env bash
    set -euo pipefail

    # Install system build dependencies (C compiler, linker, Tauri/WebView libs)
    if command -v dnf &>/dev/null; then
        echo "Installing system dependencies via dnf..."
        sudo dnf install -y gcc gcc-c++ make pkg-config \
            openssl-devel \
            gtk3-devel webkit2gtk4.1-devel libappindicator-gtk3-devel \
            librsvg2-devel patchelf
    elif command -v apt-get &>/dev/null; then
        echo "Installing system dependencies via apt..."
        sudo apt-get update
        sudo apt-get install -y build-essential pkg-config \
            libssl-dev \
            libgtk-3-dev libwebkit2gtk-4.1-dev libappindicator3-dev \
            librsvg2-dev patchelf
    else
        echo "WARNING: Unknown package manager. Ensure gcc, pkg-config, openssl-dev, and Tauri deps are installed."
    fi

    # Install Rust via rustup if missing
    if ! command -v cargo &>/dev/null; then
        echo "Installing Rust via rustup..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        source "$HOME/.cargo/env"
        echo "Rust $(rustc --version) installed."
    else
        echo "Rust already installed: $(rustc --version)"
    fi

    # Install Node.js via fnm if missing
    if ! command -v node &>/dev/null; then
        echo "Installing fnm (Fast Node Manager)..."
        curl -fsSL https://fnm.vercel.app/install | bash -s -- --skip-shell
        export PATH="$HOME/.local/share/fnm:$PATH"
        eval "$(fnm env)"
        echo "Installing Node.js LTS..."
        fnm install --lts
        fnm use lts-latest
        echo "Node $(node --version) installed."
    else
        echo "Node.js already installed: $(node --version)"
    fi

    # Install Tauri CLI
    echo "Installing tauri-cli..."
    cargo install tauri-cli

    # Install frontend dependencies
    echo "Installing frontend dependencies..."
    cd ui && npm install

    echo "Setup complete."

# Install frontend dependencies only
ui-install:
    eval "$(fnm env)" && cd ui && npm install

# ─── Build ───────────────────────────────────────────────────────────────────

# Build in debug mode
build:
    cargo build

# Build in release mode (optimized, LTO enabled)
build-release:
    cargo build --release

# Clean build artifacts
clean:
    cargo clean

# ─── Run ─────────────────────────────────────────────────────────────────────

# Run the game (Tauri desktop GUI) — installs frontend deps if missing
run:
    @eval "$(fnm env)" && test -d ui/node_modules || (echo "Installing frontend dependencies..." && cd ui && npm install)
    eval "$(fnm env)" && cargo tauri dev

# Run the game in TUI mode (terminal interface)
run-tui:
    cargo run -- --tui

# Run the game in headless REPL mode (plain stdin/stdout)
run-headless:
    cargo run -- --headless

# Run in release mode (TUI)
run-release:
    cargo run --release -- --tui

# ─── Tauri GUI ───────────────────────────────────────────────────────────────

# Start the Tauri desktop app in dev mode (frontend + backend)
tauri-dev:
    eval "$(fnm env)" && cargo tauri dev

# Build the Tauri desktop app for production
tauri-build:
    eval "$(fnm env)" && cargo tauri build

# Run the Svelte frontend dev server standalone (no Tauri backend)
ui-dev:
    eval "$(fnm env)" && cd ui && npm run dev

# Build the Svelte frontend for production
ui-build:
    eval "$(fnm env)" && cd ui && npm run build

# Run svelte-check (TypeScript + Svelte validation)
ui-check:
    eval "$(fnm env)" && cd ui && npm run check

# Run svelte-check in watch mode
ui-check-watch:
    eval "$(fnm env)" && cd ui && npm run check:watch

# Run frontend component tests (vitest)
ui-test:
    eval "$(fnm env)" && cd ui && npx vitest run

# Run Playwright E2E tests (headless Chromium, mocked Tauri IPC)
ui-e2e:
    cd ui && npx playwright test

# Update Playwright visual regression baselines
ui-e2e-update:
    cd ui && npx playwright test --update-snapshots

# Regenerate GUI screenshots via Playwright (outputs to docs/screenshots/)
screenshots:
    cd ui && npx playwright test e2e/screenshots.spec.ts

# ─── Test ────────────────────────────────────────────────────────────────────

# Run all Rust tests
test:
    cargo test

# Run a specific test by name
test-one NAME:
    cargo test {{NAME}}

# Run tests with output shown
test-verbose:
    cargo test -- --nocapture

# Run tests and generate coverage report (requires cargo-tarpaulin)
coverage:
    cargo tarpaulin --out html --output-dir target/coverage

# ─── Game Test Harness ───────────────────────────────────────────────────────

# Run the main game walkthrough test script
game-test:
    cargo run -- --script tests/fixtures/test_walkthrough.txt

# Run a specific test fixture by name (without path/extension)
game-test-one NAME:
    cargo run -- --script tests/fixtures/{{NAME}}.txt

# Run all test fixtures
game-test-all:
    @for f in tests/fixtures/*.txt; do \
        echo "=== Running $f ==="; \
        cargo run -- --script "$f" > /dev/null && echo "  PASS" || echo "  FAIL"; \
    done

# List available test fixtures
game-test-list:
    @ls tests/fixtures/*.txt | sed 's|tests/fixtures/||; s|\.txt||'

# ─── Lint & Format ──────────────────────────────────────────────────────────

# Check formatting (no changes)
fmt-check:
    cargo fmt --check

# Apply formatting
fmt:
    cargo fmt

# Run clippy linter (warnings are errors)
clippy:
    cargo clippy -- -D warnings

# Run clippy and auto-fix what it can
clippy-fix:
    cargo clippy --fix --allow-dirty -- -D warnings

# Run all checks: format, lint, and tests
check: fmt-check clippy test

# Full pre-push verification: quality gates + game harness walkthrough
verify: fmt-check clippy test game-test

# ─── Pre-commit ──────────────────────────────────────────────────────────────

# Full pre-commit suite: format, lint, test
pre-commit: fmt clippy test
    @echo "All checks passed."

# ─── Geo Tool ────────────────────────────────────────────────────────────────

# Run the geo-tool to extract OSM data for an area
geo-tool AREA:
    cargo run --bin geo-tool -- --area "{{AREA}}"

# Run the geo-tool with dry-run (preview queries only)
geo-tool-dry-run AREA:
    cargo run --bin geo-tool -- --area "{{AREA}}" --dry-run

# Run the geo-tool and merge into existing parish.json
geo-tool-merge AREA:
    cargo run --bin geo-tool -- --area "{{AREA}}" --merge data/parish.json

# ─── Documentation ───────────────────────────────────────────────────────────

# Generate and open Rust documentation
doc:
    cargo doc --open --no-deps

# Generate docs without opening browser
doc-build:
    cargo doc --no-deps

# ─── Dependencies ────────────────────────────────────────────────────────────

# Check for outdated dependencies (requires cargo-outdated)
outdated:
    cargo outdated

# Audit dependencies for security vulnerabilities (requires cargo-audit)
audit:
    cargo audit

# Update dependencies
update:
    cargo update

# ─── Docker / Container ─────────────────────────────────────────────────────

# Build the dev container image
docker-build:
    docker build -t parish-dev -f .devcontainer/Dockerfile .

# Run the game inside the dev container
docker-run:
    docker run -it --rm parish-dev

# Start a shell inside the dev container
docker-shell:
    docker run -it --rm parish-dev bash

# ─── Ollama ──────────────────────────────────────────────────────────────────

# Start the Ollama server
ollama-start:
    ollama serve &

# Pull the default model (qwen3:14b)
ollama-pull MODEL="qwen3:14b":
    ollama pull {{MODEL}}

# Check Ollama server status
ollama-status:
    @curl -sf http://localhost:11434/api/tags > /dev/null && echo "Ollama is running" || echo "Ollama is not running"

# List available Ollama models
ollama-models:
    ollama list

# ─── Agency Agents ─────────────────────────────────────────────────────

# Update Claude Code agents from github.com/msitarzewski/agency-agents
update-agents:
    #!/usr/bin/env bash
    set -euo pipefail
    TMPDIR="$(mktemp -d)"
    trap 'rm -rf "$TMPDIR"' EXIT
    echo "Cloning agency-agents..."
    git clone --depth 1 https://github.com/msitarzewski/agency-agents.git "$TMPDIR/agency-agents" 2>&1 | tail -1
    DEST=".claude/agents"
    rm -rf "$DEST"
    mkdir -p "$DEST"
    count=0
    for dir in academic design engineering game-development marketing paid-media \
               product project-management sales spatial-computing specialized \
               strategy support testing; do
        srcdir="$TMPDIR/agency-agents/$dir"
        [ -d "$srcdir" ] || continue
        for f in "$srcdir"/*.md; do
            [ -f "$f" ] || continue
            head -1 "$f" | grep -q '^---$' && cp "$f" "$DEST/" && count=$((count + 1))
        done
    done
    echo "Installed $count agents to $DEST"

# ─── Utilities ───────────────────────────────────────────────────────────────

# Count lines of Rust source code
loc:
    @find src crates/parish-core/src src-tauri/src -name '*.rs' | xargs wc -l | tail -1

# Show project tree (source only)
tree:
    @find src crates/parish-core/src src-tauri/src ui/src -type f \( -name '*.rs' -o -name '*.ts' -o -name '*.svelte' \) | sort | sed 's|[^/]*/|  |g'

# Watch for changes and rebuild (requires cargo-watch)
watch:
    cargo watch -x build

# Watch for changes and run tests (requires cargo-watch)
watch-test:
    cargo watch -x test
