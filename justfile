# Parish — An Irish Living World Text Adventure
# Run `just` or `just --list` to see all available commands.

set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

# Default: list available commands
default:
    @just --list

# ─── Build ────────────────────────────────────────────────────────────────────

# Build in debug mode
build:
    cargo build

# Build in release mode (optimized, LTO enabled)
build-release:
    cargo build --release

# Clean build artifacts
clean:
    cargo clean

# ─── Run ──────────────────────────────────────────────────────────────────────

# Run the game (debug build)
run:
    cargo run

# Run the game (release build)
run-release:
    cargo run --release

# ─── Test ─────────────────────────────────────────────────────────────────────

# Run all tests
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

# ─── Lint & Format ────────────────────────────────────────────────────────────

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

# ─── Pre-commit ───────────────────────────────────────────────────────────────

# Full pre-commit suite: format, lint, test
pre-commit: fmt clippy test
    @echo "All checks passed."

# ─── Documentation ────────────────────────────────────────────────────────────

# Generate and open Rust documentation
doc:
    cargo doc --open --no-deps

# Generate docs without opening browser
doc-build:
    cargo doc --no-deps

# ─── Dependencies ─────────────────────────────────────────────────────────────

# Check for outdated dependencies (requires cargo-outdated)
outdated:
    cargo outdated

# Audit dependencies for security vulnerabilities (requires cargo-audit)
audit:
    cargo audit

# Update dependencies
update:
    cargo update

# ─── Docker / Container ──────────────────────────────────────────────────────

# Build the dev container image
docker-build:
    docker build -t parish-dev -f .devcontainer/Dockerfile .

# Run the game inside the dev container
docker-run:
    docker run -it --rm parish-dev

# Start a shell inside the dev container
docker-shell:
    docker run -it --rm parish-dev bash

# ─── Ollama ───────────────────────────────────────────────────────────────────

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

# ─── Utilities ────────────────────────────────────────────────────────────────

# Count lines of Rust source code
loc:
    @find src -name '*.rs' | xargs wc -l | tail -1

# Show project tree (source only)
tree:
    @find src -type f -name '*.rs' | sort | sed 's|[^/]*/|  |g'

# Watch for changes and rebuild (requires cargo-watch)
watch:
    cargo watch -x build

# Watch for changes and run tests (requires cargo-watch)
watch-test:
    cargo watch -x test
