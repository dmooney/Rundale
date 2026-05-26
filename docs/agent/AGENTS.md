# docs/agent — agent scope

Deep reference directory for AI coding agents and human contributors. Invoked by the root [`AGENTS.md`](../../AGENTS.md) line _Start with the detailed agent docs in [docs/agent/README.md](README.md)_. This file is a machine-parseable index; the human-facing entry point is [`README.md`](README.md).

Proof archives, screenshots, design records, and audit artifacts live outside this directory but are cross-referenced from these docs: `docs/proofs/`, `docs/screenshots/`, `docs/adr/`, `docs/design/`, `docs/plans/`, `docs/research/`, `docs/reviews/`, `docs/audits/`.

## Scoped commands

```sh
just screenshots                                    # regenerate docs/screenshots/*.png
bash parish/scripts/check-doc-paths.sh              # validate backtick-quoted paths in docs
just agent-check                                    # proof evidence + judge verdict gate
witness-scan                                        # catch AI partial-completion markers
```

## Local gotchas

- **Docs must stay in sync with code.** The `check-doc-paths.sh` script (part of `just check` / CI `docs-consistency`) rejects backtick-quoted tokens in any doc that don't resolve to real files on disk. When renaming a file or module cited in a doc, update the doc before committing.
- **Codebase map is the starting point.** [`codebase-map.md`](codebase-map.md) provides the top-level directory index and is the first doc agents should consult when orienting. Keep its `Parish Crates` table and repository-layout table fresh.
- **Gotchas evolve with platform support.** [`gotchas.md`](gotchas.md) is the most mutation-prone file in this directory — Tokio, SQLite, Ollama, and mode-parity pitfalls change as tooling and platform support evolve.
- **Screenshots live in `docs/screenshots/`**, not in this directory. Regenerate with `just screenshots` when UI changes.
- **Proof archives live in `docs/proofs/`** — two long-lived bench areas (`local-perf/`, `rundale-bench/`) are exempt from the proof-evidence gate (rule #10). Per-task bundles go in `.proofs/<task-id>/` (gitignored).
- **Witness scan blocks merge.** Any doc that contains partial-completion markers (`[...]`, `TODO` in code blocks, unfinished sentences followed by stop-tokens) fails `witness-scan`, which gates `just check` and `just verify`.
- **Scaling guardrails (rule #11)** reference [scaling-rules.md](scaling-rules.md) for the seam review checklist. Every entry-point crate `AGENTS.md` links here — edits ripple across the workspace.
- **`act-local.md`** describes running CI with `nektos/act`. It's the source of truth for `.actrc` and the `act-*` recipes in the justfile.

## Doc index

| File | Purpose | Cross-references from |
|------|---------|----------------------|
| [`README.md`](README.md) | Human-facing table of contents for this directory | root `AGENTS.md`, entry point |
| [`build-test.md`](build-test.md) | Cargo, harness, frontend, web, and Tauri commands | `codebase-map.md`, root `AGENTS.md`, crate AGENTS.md files |
| [`architecture.md`](architecture.md) | Workspace layout, 16-crate dependency graph, module ownership | `codebase-map.md`, `code-style.md`, design docs (`docs/design/overview.md`) |
| [`code-style.md`](code-style.md) | Rust + Svelte conventions, naming, formatting, dep rules | `architecture.md`, crate AGENTS.md files |
| [`gotchas.md`](gotchas.md) | Tokio, SQLite, Ollama, mode parity, platform pitfalls | root `AGENTS.md` rule block, `parish-engine/AGENTS.md`, every crate AGENTS.md |
| [`git-workflow.md`](git-workflow.md) | Commits, tests, PR standards, review expectations | root `AGENTS.md` commit section |
| [`witness.md`](witness.md) | Witness-style completion gates (AI partial-completion markers) | `harness.md` witness row, `justfile` witness-scan recipe |
| [`agent-check.md`](agent-check.md) | PR proof-evidence gate (rule #10), local and CI source modes | root `AGENTS.md` rule #10, `harness.md`, `justfile` |
| [`skills.md`](skills.md) | Agent slash commands (`/check`, `/parish-engine`, `/task-start`, `/backlog`, ...) | root `AGENTS.md` skill list, `harness.md` skills section, `.agents/skills/` |
| [`harness.md`](harness.md) | One-page map of every sensor, skill, gate — what fires when | root `AGENTS.md`, `codebase-map.md`, `agent-check.md`, `skills.md`, `witness.md` |
| [`act-local.md`](act-local.md) | Running CI workflows locally with `nektos/act` | `justfile` act-* recipes, `.actrc` |
| [`idempotency.md`](idempotency.md) | `Idempotency-Key` header support (#619) on mutating routes | `parish-server/AGENTS.md`, scaling review (#614–#622) |
| [`scaling-rules.md`](scaling-rules.md) | Scaling guardrails — per-session state, seam review checklist (rule #11) | root `AGENTS.md` rule #11, `parish-core/AGENTS.md`, `parish-server/AGENTS.md` |
| [`codebase-map.md`](codebase-map.md) | Top-level directory index, Parish crate table, entry points | root `AGENTS.md`, every AGENTS.md file across workspace |
| [`triage-vocabulary.md`](triage-vocabulary.md) | Canonical labels for issue triage (priorities + themes) | `.github/triage-labels.json`, `/backlog` skill |
| [`tracing.md`](tracing.md) | `tracing` / OpenTelemetry OTLP conventions (#621) | `parish-server/AGENTS.md`, scaling review |
