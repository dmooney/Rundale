# Parish — An Irish Living World Text Adventure
# Run `just` or `just --list` to see all available commands.

set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

# Default: list available commands
default:
    @just --list

# ─── Setup ───────────────────────────────────────────────────────────────────

# One-time developer setup: install tools and frontend dependencies
setup:
    cargo install tauri-cli
    cd ui && npm install

# Install frontend dependencies only
ui-install:
    cd ui && npm install

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

# Run the game (Tauri desktop GUI)
run:
    cargo tauri dev

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
    cargo tauri dev

# Build the Tauri desktop app for production
tauri-build:
    cargo tauri build

# Run the Svelte frontend dev server standalone (no Tauri backend)
ui-dev:
    cd ui && npm run dev

# Build the Svelte frontend for production
ui-build:
    cd ui && npm run build

# Run svelte-check (TypeScript + Svelte validation)
ui-check:
    cd ui && npm run check

# Run svelte-check in watch mode
ui-check-watch:
    cd ui && npm run check:watch

# Run frontend component tests (vitest)
ui-test:
    cd ui && npx vitest run

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
