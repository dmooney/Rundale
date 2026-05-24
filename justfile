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

# Profile inference request volume during a human-paced local-inference demo run
demo-profile DURATION="300" PAUSE="10" MODEL="mlx-community/Qwen2.5-14B-Instruct-4bit" UPSTREAM="http://localhost:8000/v1":
    cd parish && just demo-profile {{DURATION}} {{PAUSE}} {{MODEL}} {{UPSTREAM}}

# Run the axum web server
web PORT="3001":
    cd parish && just web {{PORT}}

# ─── Quality Gates ──────────────────────────────────────────────────────────

# Pre-commit gate: format, lint, tests, placeholder scan, doc-paths
check:
    cd parish && just check

# Agent proof gate (local mode): validates the bundle in .proofs/<task-id>/
# against the same rules CI uses. Pass `--source=pr <num>` to validate a
# PR comment instead (what CI does).
agent-check *ARGS:
    bash parish/scripts/agent-check.sh {{ARGS}}

# Attach a proof bundle to a PR as a structured comment. The bundle lives
# at .proofs/<TASK_ID>/ (gitignored). Idempotent — re-running edits the
# existing parish-proof-bundle comment instead of appending. PR_NUM
# defaults to the PR for the current branch.
attach-proof TASK_ID PR_NUM="":
    bash parish/scripts/attach-proof.sh {{TASK_ID}} {{PR_NUM}}

# Pre-push gate: check + game harness walkthrough
verify:
    cd parish && just verify

# Run the full Rundale dialect-model training pipeline on RunPod (provisions pod, runs SFT + DPO + dialect oracle, packages GGUF, runs /prove, tears down). See docs/design/gemma4-rundale-training-plan.md
train-rundale-dialect:
    uv run --project training python training/scripts/orchestrate.py

# Run all Rust tests
test:
    cd parish && just test

# Run tests and generate coverage report
coverage:
    cd parish && just coverage

# Run Rust coverage and fail if it drops below the current ratchet floor
coverage-check:
    cd parish && just coverage-check

# Witness-style deterministic scan for AI partial-completion markers
witness-scan:
    cd parish && just witness-scan

# Regenerate gameplay-eval baselines after intentional gameplay change
baselines:
    cd parish && just baselines

# Run the rundale-bench Gaeilge slice for one OpenAI-compatible target.
eval-gaeilge TARGET LIMIT="":
    #!/usr/bin/env bash
    set -euo pipefail
    args=(--target "{{TARGET}}" --suite v1 --slice gaeilge)
    if [ -n "{{LIMIT}}" ]; then
      args+=(--limit "{{LIMIT}}")
    fi
    python3 rundale-bench/rundale_bench.py "${args[@]}"

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

# Reset first-run onboarding (BYOK wizard). Pass extra args: --config, --keys, --all, --dry-run.
reset-onboarding *ARGS:
    bash parish/scripts/reset-onboarding.sh {{ARGS}}

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

# Check for outdated dependencies
outdated:
    cd parish && just outdated

# Audit dependencies for security vulnerabilities
audit:
    cd parish && just audit

# Run inference benchmark harness against any OpenAI-compatible endpoint.
# Override via env: BASE_URL, INTENT_MODEL, MAIN_MODEL, ITERS, API_KEY.
inf-bench:
    cd parish && just inf-bench

# Build the bundled portable Python + vllm-mlx venv shipped inside the
# macOS .app's Contents/Resources for first-run local inference. Output:
# parish/dist/vllm-mlx-bundle.tar.zst (~80-100 MB compressed).
#
# Requires:
#   - macOS aarch64 (Apple Silicon) — the only target Parish ships local
#     inference for; Linux/Windows use Ollama and don't need this bundle.
#   - curl, tar, zstd, sh (in PATH)
#   - Internet (downloads python-build-standalone + pip-installs vllm-mlx)
#
# CI (`.github/workflows/build-vllm-mlx-bundle.yml`) drives this on
# macos-14 runners; dev runs locally before `cargo tauri build`.
build-vllm-mlx-bundle:
    cd parish && just build-vllm-mlx-bundle

# ─── Release ────────────────────────────────────────────────────────────────

# Bump versions, commit, and tag a release locally. See docs/release.md.
release VERSION:
    cd parish && just release {{VERSION}}

# Dry-run the release: show would-be diffs without writing or tagging.
release-dry-run VERSION:
    cd parish && just release-dry-run {{VERSION}}
