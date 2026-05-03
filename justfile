# Rundale — An Irish Living World Text Adventure
# Top-level Justfile to proxy commands to the Parish engine.
# Run `just` or `just --list` to see all available commands.

set shell := ["bash", "-euo", "pipefail", "-c"]

# Default: list available commands
default:
    @just --list

# ─── Setup ───────────────────────────────────────────────────────────────────

# One-time developer setup: install system deps, Rust, Node, and frontend packages
setup:
    cd parish && just setup

# ─── Parish Engine Proxies ──────────────────────────────────────────────────

# Build the workspace
build:
    cd parish && just build

# Build the workspace in release mode
build-release:
    cd parish && just build-release

# Run the game (Tauri desktop GUI)
run:
    cd parish && just run

# Run the game in headless REPL mode
run-headless:
    cd parish && just run-headless

# Run the LLM demo / auto-player (optional: PAUSE=seconds MAX_TURNS=n)
demo PAUSE="2" MAX_TURNS="":
    cd parish && just demo {{PAUSE}} {{MAX_TURNS}}

# Run the axum web server
web PORT="3001":
    cd parish && just web {{PORT}}

# ─── Quality Gates ──────────────────────────────────────────────────────────

# Pre-commit gate: format, lint, tests, placeholder scan, doc-paths
check:
    cd parish && just check

# Agent proof gate: requires changed proof evidence and judge verdicts for proof-relevant PRs
agent-check:
    bash parish/scripts/agent-check.sh

# Pre-push gate: check + game harness walkthrough
verify:
    cd parish && just verify

# Run all Rust tests
test:
    cd parish && just test

# Witness-style deterministic scan for AI partial-completion markers
witness-scan:
    cd parish && just witness-scan

# Regenerate gameplay-eval baselines after intentional gameplay change
baselines:
    cd parish && just baselines

# Read-only audit of gameplay fixture coverage
harness-audit:
    cd parish && just harness-audit

# Run frontend component tests
ui-test:
    cd parish && just ui-test

# Run Playwright E2E tests
ui-e2e:
    cd parish && just ui-e2e

# Regenerate GUI screenshots via Playwright
screenshots:
    cd parish && just screenshots

# ─── Utilities ───────────────────────────────────────────────────────────────

# Run the main game walkthrough test script
game-test:
    cd parish && just game-test

# Run a specific test fixture by name
game-test-one NAME:
    cd parish && just game-test-one {{NAME}}

# Run all test fixtures
game-test-all:
    cd parish && just game-test-all

# List all commands available in the parish engine
parish-help:
    cd parish && just --list

# Regenerate third-party notice files
notices:
    cd parish && just notices

# Audit dependencies for security vulnerabilities
audit:
    cd parish && just audit
