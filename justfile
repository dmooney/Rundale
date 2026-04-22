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
    cd apps/ui && npm install

    echo "Setup complete."

# Install frontend dependencies only
ui-install:
    eval "$(fnm env)" && cd apps/ui && npm install

# ─── Build ───────────────────────────────────────────────────────────────────

# Build the workspace in debug mode
build:
    cargo build

# Build the workspace in release mode (optimized, LTO enabled)
build-release:
    cargo build --release

# Clean build artifacts
clean:
    cargo clean

# ─── Run ─────────────────────────────────────────────────────────────────────

# Run the game (Tauri desktop GUI) — installs frontend deps if missing.
# Auto-detects a free dev port so multiple instances can run simultaneously.
run:
    #!/usr/bin/env bash
    eval "$(fnm env)"
    test -d apps/ui/node_modules || (echo "Installing frontend dependencies..." && cd apps/ui && npm install)
    PORT=5173
    while ss -tln 2>/dev/null | grep -q ":$PORT " || lsof -iTCP:$PORT -sTCP:LISTEN >/dev/null 2>&1; do
        PORT=$((PORT + 1))
        if [ "$PORT" -gt 5200 ]; then echo "No free port found in range 5173-5200" >&2; exit 1; fi
    done
    export PARISH_DEV_PORT=$PORT
    if [ "$PORT" -eq 5173 ]; then
        echo "Dev server port: $PORT"
        cargo tauri dev
    else
        echo "Dev server port: $PORT (default 5173 was in use)"
        cargo tauri dev --config "{\"build\":{\"devUrl\":\"http://localhost:$PORT\"}}"
    fi

# Run the game in headless REPL mode (plain stdin/stdout)
run-headless:
    cargo run -p parish -- --headless

# Run the axum web server (serves the Svelte UI in a browser)
#
# Always rebuilds the frontend first with a clean cache, because Vite's
# `.svelte-kit/` cache can hold stale compiled output after git operations
# (rebase/checkout/merge), making source edits appear invisible in the served
# UI and spuriously re-emitting already-fixed build warnings.
web PORT="3001": ui-build
    cargo run -p parish -- --web {{PORT}}

# ─── Tauri & Frontend ────────────────────────────────────────────────────────

# Start the Tauri desktop app in dev mode (same as `run`)
tauri-dev:
    just run

# Build the Tauri desktop app for production
tauri-build:
    eval "$(fnm env)" && cargo tauri build

# Run the Svelte frontend dev server standalone (no Tauri backend)
ui-dev:
    eval "$(fnm env)" && cd apps/ui && npm run dev

# Build the Svelte frontend for production
#
# Clears `.svelte-kit/` and `dist/` before building so Vite can't serve
# stale compiled output after git operations (rebase/checkout/merge) that
# shuffle file mtimes in ways that confuse Vite's cache invalidation.
ui-build:
    eval "$(fnm env)" && cd apps/ui && rm -rf .svelte-kit dist && npm run build

# Run svelte-check (TypeScript + Svelte validation)
ui-check:
    eval "$(fnm env)" && cd apps/ui && npm run check

# Run frontend component tests (vitest)
ui-test:
    eval "$(fnm env)" && cd apps/ui && npx vitest run

# Run Playwright E2E tests (headless Chromium, mocked Tauri IPC)
ui-e2e:
    cd apps/ui && npx playwright test

# Update Playwright visual regression baselines
ui-e2e-update:
    cd apps/ui && npx playwright test --update-snapshots

# Regenerate GUI screenshots via Playwright (outputs to docs/screenshots/)
screenshots:
    cd apps/ui && npx playwright test e2e/screenshots.spec.ts

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
    cargo run -p parish -- --script testing/fixtures/test_walkthrough.txt

# Run a specific test fixture by name (without path/extension)
game-test-one NAME:
    cargo run -p parish -- --script testing/fixtures/{{NAME}}.txt

# Run all test fixtures
game-test-all:
    @for f in testing/fixtures/*.txt; do \
        echo "=== Running $f ==="; \
        cargo run -p parish -- --script "$f" > /dev/null && echo "  PASS" || echo "  FAIL"; \
    done

# List available test fixtures
game-test-list:
    @ls testing/fixtures/*.txt | sed 's|testing/fixtures/||; s|\.txt||'

# ─── Lint, Format & Quality Gates ────────────────────────────────────────────

# Check formatting (no changes)
fmt-check:
    cargo fmt --check

# Apply formatting
fmt:
    cargo fmt

# Run clippy linter (warnings are errors)
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Run clippy and auto-fix what it can
clippy-fix:
    cargo clippy --fix --allow-dirty --all-targets -- -D warnings

# Pre-commit gate: format, lint, tests
check: fmt-check clippy test

# Pre-push gate: check + game harness walkthrough
verify: fmt-check clippy test game-test

# ─── Geo Tool ────────────────────────────────────────────────────────────────

# Run the geo-tool to extract OSM data for an area
geo-tool AREA:
    cargo run -p geo-tool -- --area "{{AREA}}"

# Run the geo-tool with dry-run (preview queries only)
geo-tool-dry-run AREA:
    cargo run -p geo-tool -- --area "{{AREA}}" --dry-run

# Run the geo-tool and merge into the active mod's world.json
geo-tool-merge AREA:
    cargo run -p geo-tool -- --area "{{AREA}}" --merge mods/rundale/world.json

# Build the real-coordinate alignment utility binary
realign-coords-build:
    cargo build -p geo-tool --bin realign_rundale_coords

# Run the real-coordinate alignment utility against Rundale (in place)
realign-coords:
    cargo run -p geo-tool --bin realign_rundale_coords -- --world mods/rundale/world.json --in-place

# Run the real-coordinate alignment utility with custom args
realign-coords-run *ARGS:
    cargo run -p geo-tool --bin realign_rundale_coords -- {{ARGS}}

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

# ─── Ollama ──────────────────────────────────────────────────────────────────

# Start the Ollama server in the background
ollama-start:
    ollama serve &

# Pull a model (default: qwen3:14b)
ollama-pull MODEL="qwen3:14b":
    ollama pull {{MODEL}}

# Check Ollama server status
ollama-status:
    @curl -sf http://localhost:11434/api/tags > /dev/null && echo "Ollama is running" || echo "Ollama is not running"

# List installed Ollama models
ollama-models:
    ollama list
