# docs/agent — agent scope

Reference directory for AI coding agents and human contributors. Human-facing entry point is [`README.md`](README.md). Historical proof archives live in the ignored, iCloud-backed local `docs/proofs/` directory; screenshots and design artifacts live in `docs/screenshots/`, `docs/adr/`, `docs/design/`, `docs/plans/`, `docs/research/`, `docs/reviews/`, and `docs/audits/`.

## Scoped commands

```sh
just screenshots                                    # regenerate docs/screenshots/*.png
bash parish/scripts/check-doc-paths.sh              # validate backtick-quoted paths in docs
just agent-check                                    # proof evidence + judge verdict gate
witness-scan                                        # catch AI partial-completion markers
```

## Local gotchas

- **Docs must stay in sync with code.** `check-doc-paths.sh` (part of `just check` / CI `docs-consistency`) rejects broken relative Markdown links across active docs and nonexistent backtick-quoted agent paths. Update the doc before committing any file or module rename it cites.
- **Start orientation at [`codebase-map.md`](codebase-map.md).** Keep its `Parish Crates` table and repository-layout table fresh.
- **[`gotchas.md`](gotchas.md) is the most mutation-prone file** — Tokio, SQLite, Ollama, and mode-parity pitfalls change as tooling evolves.
- **Screenshots live in `docs/screenshots/`** — regenerate with `just screenshots` on UI changes.
- **Proof archives in local `docs/proofs/`** — ignored by Git and expected to resolve to the iCloud-backed archive. Per-task bundles go in `.proofs/<task-id>/` (also gitignored); publish concise hashes and summaries in tracked docs or PR bodies.
- **Witness scan blocks merge.** Docs with partial-completion markers (`[...]`, `TODO` in code blocks, unfinished sentences before stop-tokens) fail `witness-scan`, which gates `just check` and `just verify`.
- **Scaling guardrails (rule #11)** are in [scaling-rules.md](scaling-rules.md). Every entry-point crate AGENTS.md links here — edits ripple across the workspace.
- **[`act-local.md`](act-local.md)** is the source of truth for `.actrc` and the `act-*` justfile recipes.

## Doc index

| File                                           | Purpose                                                                   | Cross-references from                                                            |
| ---------------------------------------------- | ------------------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| [`README.md`](README.md)                       | Human-facing table of contents for this directory                         | root `AGENTS.md`, entry point                                                    |
| [`build-test.md`](build-test.md)               | Cargo, harness, frontend, web, and Tauri commands                         | `codebase-map.md`, root `AGENTS.md`, crate AGENTS.md files                       |
| [`architecture.md`](architecture.md)           | Workspace layout, crate dependency graph, module ownership                | `codebase-map.md`, `code-style.md`, design docs (`docs/design/overview.md`)      |
| [`code-style.md`](code-style.md)               | Rust + Svelte conventions, naming, formatting, dep rules                  | `architecture.md`, crate AGENTS.md files                                         |
| [`gotchas.md`](gotchas.md)                     | Tokio, SQLite, Ollama, mode parity, platform pitfalls                     | root `AGENTS.md` rule block, `parish-engine/AGENTS.md`, every crate AGENTS.md    |
| [`git-workflow.md`](git-workflow.md)           | Commits, tests, PR standards, review expectations                         | root `AGENTS.md` commit section                                                  |
| [`improvement-drain.md`](improvement-drain.md) | Event-driven portfolio, readiness contract, WIP and authoritative linkage | `triage-vocabulary.md`, `.github/workflows/triage-audit.yml`, issue/PR templates |
| [`witness.md`](witness.md)                     | Witness-style completion gates (AI partial-completion markers)            | `harness.md` witness row, `justfile` witness-scan recipe                         |
| [`agent-check.md`](agent-check.md)             | PR proof-evidence gate (rule #10), local and CI source modes              | root `AGENTS.md` rule #10, `harness.md`, `justfile`                              |
| [`skills.md`](skills.md)                       | Agent slash commands (`/check`, `/parish-engine`, `/backlog`, ...)        | root `AGENTS.md` skill list, `harness.md` skills section, `.agents/skills/`      |
| [`harness.md`](harness.md)                     | One-page map of every sensor, skill, gate — what fires when               | root `AGENTS.md`, `codebase-map.md`, `agent-check.md`, `skills.md`, `witness.md` |
| [`act-local.md`](act-local.md)                 | Running CI workflows locally with `nektos/act`                            | `justfile` act-\* recipes, `.actrc`                                              |
| [`idempotency.md`](idempotency.md)             | `Idempotency-Key` header support (#619) on mutating routes                | `parish-server/AGENTS.md`, scaling review (#614–#622)                            |
| [`scaling-rules.md`](scaling-rules.md)         | Scaling guardrails — per-session state, seam review checklist (rule #11)  | root `AGENTS.md` rule #11, `parish-core/AGENTS.md`, `parish-server/AGENTS.md`    |
| [`codebase-map.md`](codebase-map.md)           | Top-level directory index, Parish crate table, entry points               | root `AGENTS.md`, every AGENTS.md file across workspace                          |
| [`triage-vocabulary.md`](triage-vocabulary.md) | Canonical labels for issue triage (priorities + themes)                   | `.github/triage-labels.json`, `/backlog` skill                                   |
| [`tracing.md`](tracing.md)                     | `tracing` / OpenTelemetry OTLP conventions (#621)                         | `parish-server/AGENTS.md`, scaling review                                        |
