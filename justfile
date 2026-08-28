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
    just install-hooks

# Point git at the versioned hooks in .githooks/ (idempotent). The pre-push
# hook runs the docs/data format check so broken Markdown never reaches a PR.
install-hooks:
    git config core.hooksPath .githooks
    @echo "git hooks installed: core.hooksPath -> .githooks (pre-push docs-format gate active)"

# Install the language servers Claude Code uses for Svelte/TS symbol navigation
setup-lsp:
    cd parish && just setup-lsp

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

# Enforce generated-output, large-file, and documentation-screenshot policy.
repository-artifacts:
    bash parish/scripts/check-repository-artifacts.sh

# Attach a proof bundle to a PR. The bundle lives at .proofs/<TASK_ID>/
# (gitignored). By default it is written into the PR body (race-free) and is
# idempotent. Extra args pass through: a PR number (defaults to the current
# branch's PR) and/or a mode flag — --as-comment (legacy) or --via-mcp
# (no-gh sandbox: emits the block on stdout for posting via the GitHub MCP).
# E.g. `just attach-proof 1178 --via-mcp` or `just attach-proof 1178 42`.
attach-proof TASK_ID *ARGS:
    bash parish/scripts/attach-proof.sh {{TASK_ID}} {{ARGS}}

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

# Run the harness corpus in differential (shadow) mode and summarize the
# legacy-vs-real-game_loop divergence ledger. Non-gating measurement (#1159).
harness-shadow *ARGS:
    bash parish/scripts/harness-shadow.sh {{ARGS}}

# Run frontend component tests
ui-test:
    cd parish && just ui-test

# Lint the frontend (ESLint)
ui-lint:
    cd parish && just ui-lint

# Auto-format the frontend (Prettier)
ui-format:
    cd parish && just ui-format

# Check frontend formatting without writing
ui-format-check:
    cd parish && just ui-format-check

# Run Playwright E2E tests
ui-e2e:
    cd parish && just ui-e2e

# Regenerate GUI screenshots via Playwright
screenshots:
    cd parish && just screenshots

# ─── Repo-wide docs/data formatting (Prettier + markdownlint) ─────────────────

# Auto-format docs/data (Markdown, JSON, YAML) repo-wide
fmt-docs:
    eval "$(fnm env)" && npm run format

# Check docs/data formatting without writing (CI gate)
fmt-docs-check:
    eval "$(fnm env)" && npm run format:check

# Lint Markdown (markdownlint-cli2)
lint-docs:
    eval "$(fnm env)" && npm run lint:md

# ─── Python tooling (ruff + mypy + yamllint + pytest) ─────────────────────────
# Recipes prefer the local .venv-dev (just setup-py) and fall back to PATH tools.

# Create/refresh the Python dev virtualenv from requirements-dev.txt
setup-py:
    python3 -m venv .venv-dev
    .venv-dev/bin/pip install --quiet --upgrade pip
    .venv-dev/bin/pip install --quiet -r requirements-dev.txt

# Resolve a tool from .venv-dev if present, else PATH
_py-bin TOOL:
    @if [ -x ".venv-dev/bin/{{TOOL}}" ]; then echo ".venv-dev/bin/{{TOOL}}"; else echo "{{TOOL}}"; fi

# Auto-format Python (ruff format)
fmt-py:
    "$(just _py-bin ruff)" format .

# Lint + type-check + yaml-lint Python (CI gate)
lint-py:
    "$(just _py-bin ruff)" check .
    "$(just _py-bin ruff)" format --check .
    "$(just _py-bin mypy)" .
    "$(just _py-bin yamllint)" .

# Run the Python (bench) test suite
test-py:
    "$(just _py-bin pytest)"

# ─── Shell tooling (shellcheck + shfmt) ───────────────────────────────────────
# shfmt flags (-i 4 -ci -bn) mirror .editorconfig so editors agree.

# Auto-format all shell scripts (shfmt, writes in place)
fmt-shell:
    shfmt -i 4 -ci -bn -w $(git ls-files '*.sh')

# Lint + format-check all shell scripts (CI gate)
lint-shell:
    shellcheck -S warning $(git ls-files '*.sh')
    shfmt -i 4 -ci -bn -d $(git ls-files '*.sh')

# ─── TOML tooling (taplo) ─────────────────────────────────────────────────────
# Config in taplo.toml (excludes intentionally-malformed test fixtures).

# Auto-format all TOML (taplo, writes in place)
fmt-toml:
    taplo fmt

# Lint + format-check all TOML (CI gate)
lint-toml:
    taplo fmt --check
    taplo lint

# ─── Aggregate non-Rust gates ─────────────────────────────────────────────────

# Format every non-Rust file type in place (web, docs/data, python, shell, toml)
fmt-all: ui-format fmt-docs fmt-py fmt-shell fmt-toml
    @echo "Formatted web + docs/data + python + shell + toml."

# Run every non-Rust lint/format gate (CI parity, non-mutating)
lint-all: ui-lint ui-format-check lint-docs lint-py lint-shell lint-toml
    @echo "All non-Rust quality gates passed."

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
