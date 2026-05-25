Plan a naming + structural refactor for the Parish workspace's entry-point crates. Goal: eliminate misleading binary/crate names and align directory names with package names. Pure relocation + renames — no behavior changes.

## Current state (verified)

Workspace at `parish/crates/`:

| Dir | Cargo `name` | Binary | Lib | What it actually does |
|---|---|---|---|---|
| `parish-cli/` | `parish-repl` | `parish-repl` | `parish` | Three muxed modes: `--headless` (stdin REPL), `--web PORT` (calls `parish_server::run_server`), no flag (Tauri desktop). Engine in-process. |
| `parish-server/` | `parish-server` | **none — lib only** | `parish-server` | Axum routes + auth + middleware + `sync_routes` + `ws`. Exports `pub async fn run_server(port, data_dir, static_dir)`. Cannot be started directly. |
| `parish-client/` | `parish-client` | `parish` | — | New in PR #1043. Thin HTTP shell. Calls `POST /api/command` / `GET /api/state`. Four modes: single-shot, `--script`, `--json`, REPL. |
| `parish-tauri/` | `parish-tauri` | `parish-tauri` | `parish-tauri` | Tauri 2 desktop wrapper. |

Default workspace member: `crates/parish-cli` (so `cargo build` builds `parish-repl`).

## Problems

1. **`parish-repl` is a misnomer.** REPL is one of three modes; `--web` is a server, `--script` is batch. The "REPL" name actively misleads — user just got confused reading "parish-repl --web".
2. **`parish-server` is a misnomer.** Sounds like a binary; is a lib. The only way to actually start a server is through the misnamed `parish-repl --web`.
3. **Directory `parish-cli/` no longer matches its package name** `parish-repl`. Pure cruft from the rename in #1043.
4. **Three modes muxed in one binary** force unrelated dependencies and flags into a single CLI surface. `--headless` and `--web` have nothing in common beyond engine init.
5. **`parish-client` is the only honestly-named entry-point crate.** Reflexively, the rest deserve the same treatment.

## Constraints

- No behavior changes. Pure rename / split / move.
- Don't break MCP, CI, hooks, justfile recipes, `.proofs` machinery, or `agent-check` path globs (`parish-cli/**` etc. — these are in AGENTS.md rule #10 live-proof tier).
- Architecture-fitness tests at `parish/crates/parish-core/tests/architecture_fitness.rs` enforce module ownership against specific crate names — update them in lock-step.
- `parish-client`'s wire types depend on `parish-server::sync_types` — keep them in sync (a recent bug from PR #1043 verification — see commit `2f754f90`).
- Rule #12 (cross-runtime orchestration in `parish-core`) names the three entry-point crates by name — update.
- `default-members` in `parish/Cargo.toml` currently `["crates/parish-cli"]`.

## Options to evaluate

**A. Rename-only (cheap):**
- Rename binary `parish-repl` → `parish-engine` (or `parish-host`).
- Rename dir `parish-cli/` → match new crate name.
- Keep three-mode mux.
- `parish-server` stays lib-only. Rename to `parish-axum`? Or leave alone?

**B. Split entry points (cleaner):**
- Give `parish-server` its own `main.rs` so it boots directly (`cargo run -p parish-server -- --port 3001`). Lib stays usable for embedding.
- Remove `--web` from the in-process binary; rename it to `parish-engine` (handles `--headless` + Tauri-launch only).
- Result: each entry-point crate has one job — `parish-engine` (in-process), `parish-server` (HTTP server), `parish-tauri` (desktop), `parish-client` (thin HTTP shell).

**C. Extract shared CLI layer:**
- New `parish-cli` crate (real one this time): argument parsing, output renderer, REPL loop, script driver.
- Consumed by both `parish-engine` (in-process) and `parish-client` (HTTP).
- Kills duplication between `parish-cli/src/headless.rs` and `parish-client/src/repl.rs`.

**D. Status quo + docs only.** (Already partially done — see commits `0e55cab5`, `cb5c0434`.)

## Deliverable

A phased plan PR that:
1. Decides which combination (A, B, C, or hybrid) is worth doing.
2. Sequences the renames/splits so each commit compiles and passes tests.
3. Names every file touched: `Cargo.toml` (workspace `members` + `default-members`), every crate's `Cargo.toml`, architecture-fitness test, AGENTS.md, README.md, docs/agent/*.md, parish/justfile, root justfile, `.github/workflows/**`, `.claude/hooks/**` (particularly the agent-check / Stop hooks that pattern-match crate names), `parish/scripts/parish-mcp-backend.sh`, parish-mcp README, plus anywhere in source code that uses `parish_cli::` or `parish_server::run_server` directly.
4. Identifies which renames are git-tracked moves (preserves blame) vs new files.
5. Calls out backwards-compat: cargo aliases (workspace `[workspace.metadata]` or shell aliases) for the old binary names, if any.
6. Verification plan: `cargo build --workspace`, `cargo test --workspace --exclude parish-tauri`, `just check`, `just web` smoke test, `just run-client` smoke test, and a manual `mcp__parish__*` round-trip against the renamed server binary.

Optimize for: shipping in 1-3 commits, no behavior drift, no test regressions, and a doc set that finally matches reality.

Return: phased commit list, key files per phase, and the recommended option with rationale. Under 800 words.
