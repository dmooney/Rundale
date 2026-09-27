# Codebase Map

One-page navigation index for the repository. It lists checked-in top-level
directories, the Limerick workspace roots, and local/generated directories agents
are likely to see. Scoped instructions live in `AGENTS.md`; `CLAUDE.md` is a
symlink where present.

## Repository Layout

| Path                                              | Purpose                                                                      | Entry / key file                        | Scope doc                                     |
| ------------------------------------------------- | ---------------------------------------------------------------------------- | --------------------------------------- | --------------------------------------------- |
| `AGENTS.md`, `CLAUDE.md`                          | Repo-wide agent instructions. `CLAUDE.md` is a symlink to `AGENTS.md`        | [AGENTS.md](../../AGENTS.md)            | [AGENTS.md](../../AGENTS.md)                  |
| `LEARNINGS.md`                                    | Short-lived gotchas and surprising defaults for future agents                | [LEARNINGS.md](../../LEARNINGS.md)      | -                                             |
| `limerick/`                                       | Main Rust workspace and frontend workspace for the Limerick engine           | [Cargo.toml](../../limerick/Cargo.toml) | -                                             |
| `limerick/crates/`                                | 24 Rust workspace crates: binaries, composition crate, and leaf logic crates | see [Limerick crates](#limerick-crates) | per crate                                     |
| `limerick/apps/ui/`                               | Svelte 5 + TypeScript frontend shared by desktop and web modes               | `src/routes/`, `src/lib/`               | [AGENTS.md](../../limerick/apps/ui/AGENTS.md) |
| `limerick/testing/`                               | Asserted scenarios, legacy fixtures, proof scripts, evals, and test data     | `scenarios/`, `fixtures/`, `proofs/`    | [AGENTS.md](../../limerick/testing/AGENTS.md) |
| `limerick/scripts/`                               | Check, proof, MCP-backend, screenshot, and release helper scripts            | `*.sh`, `*.py`                          | -                                             |
| `limerick/assets/`                                | Bundled app assets such as fonts                                             | `fonts/`                                | -                                             |
| `limerick/dist/`                                  | Runtime distribution helpers and local model/proxy assets                    | `vllm-mlx/`                             | -                                             |
| `mods/`                                           | Game/content mods, provider mods, and settings mods                          | `mod-list.toml`, provider dirs          | -                                             |
| `mods/rundale/`                                   | Rundale game content: NPCs, world, prompts, palette, and mod metadata        | `mod.toml`                              | [AGENTS.md](../../mods/rundale/AGENTS.md)     |
| `mods/testbed/`                                   | Small settings/content mod for deterministic tests                           | `mod.toml`                              | -                                             |
| `rundale-bench/`                                  | v1 dialogue benchmark, candidate configs, and bench artifacts                | `candidates_*.toml`, `artifacts/`       | -                                             |
| `promptfoo/`                                      | v2 benchmark of record + generated GitHub Pages site (`bench-site/`)         | `leaderboard/`, `bench-site/`           | -                                             |
| `docs/`                                           | Project documentation hub                                                    | [`index.md`](../index.md)               | -                                             |
| `docs/agent/`                                     | Agent-facing engineering docs                                                | [`README.md`](README.md)                | -                                             |
| `docs/graphics-v2/`                               | Visual-client research, art provenance, and reproducible rendering evidence  | [`README.md`](../graphics-v2/README.md) | [AGENTS.md](../graphics-v2/AGENTS.md)         |
| `docs/proofs/`                                    | Ignored local/iCloud proof archives (`local-perf/`, `rundale-bench/`)        | -                                       | -                                             |
| `docs/screenshots/`                               | Current, referenced documentation images                                     | `*.png`                                 | -                                             |
| `docs/adr/`, `docs/design/`, `docs/plans/`        | Architecture records, design notes, and planning docs                        | `*.md`                                  | -                                             |
| `docs/research/`, `docs/reviews/`, `docs/audits/` | Research notes, review artifacts, and audits                                 | `*.md`                                  | -                                             |
| `deploy/`                                         | Packaging and release artifacts                                              | `Dockerfile`                            | -                                             |
| `scripts/`                                        | Root-level utility scripts outside the Limerick workspace                    | `loc_projection.py`                     | -                                             |
| `crates/`                                         | Root-level Rust examples/experiments outside the Limerick workspace          | `limerick-world/examples/`              | -                                             |
| `.agents/`                                        | Tool-agnostic agent assets and source skills                                 | `skills/`                               | -                                             |
| `.claude/`                                        | Claude Code hooks, commands, agents, and local settings                      | `settings.json`, `hooks/`               | -                                             |
| `.claude-plugin/`                                 | Distributable Rundale plugin manifest                                        | `plugin.json`                           | -                                             |
| `.codex/`                                         | Codex project skill/config assets                                            | `skills/`                               | -                                             |
| `.opencode/`                                      | opencode agents, commands, skills, tools, and plugin config                  | `opencode.jsonc`, `skills/`             | -                                             |
| `.github/`                                        | GitHub workflows, commands, labels, and PR template                          | `workflows/`                            | -                                             |
| `.devcontainer/`                                  | Dev container image and editor setup                                         | `devcontainer.json`                     | -                                             |
| `.vscode/`                                        | Workspace editor tasks, launch configs, and settings                         | `settings.json`, `tasks.json`           | -                                             |

## Limerick Crates

The Limerick workspace currently has 24 crates under `limerick/crates/`.

| Path                                    | Purpose                                                                                                                                                                                                           | Entry / key file                                                             | Scope doc                                                       |
| --------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------- | --------------------------------------------------------------- |
| `limerick/crates/limerick-engine/`      | Binary `limerick-engine`: `--headless`, `--script FILE`, and Tauri-default engine-in-process launch                                                                                                               | `src/main.rs`                                                                | -                                                               |
| `limerick/crates/limerick-client/`      | Binary `limerick`: thin HTTP client for `limerick-server` (`POST /api/command`, `GET /api/state`)                                                                                                                 | `src/main.rs`, `src/client.rs`                                               | -                                                               |
| `limerick/crates/limerick-server/`      | Axum HTTP/WebSocket web backend. Library (`run_server`) plus binary (`cargo run -p limerick-server -- --port 3001`)                                                                                               | `src/main.rs`, `src/lib.rs`                                                  | [AGENTS.md](../../limerick/crates/limerick-server/AGENTS.md)    |
| `limerick/crates/limerick-tauri/`       | Desktop app shell and MCP bridge                                                                                                                                                                                  | `src/lib.rs`, `src/mcp_bridge.rs`                                            | [AGENTS.md](../../limerick/crates/limerick-tauri/AGENTS.md)     |
| `limerick/crates/limerick-core/`        | Backend-agnostic composition crate and shared orchestration                                                                                                                                                       | `src/lib.rs`                                                                 | [AGENTS.md](../../limerick/crates/limerick-core/AGENTS.md)      |
| `limerick/crates/limerick-config/`      | Game, provider, runtime, and flag configuration                                                                                                                                                                   | `src/lib.rs`                                                                 | -                                                               |
| `limerick/crates/limerick-providers/`   | LLM transport: provider HTTP clients, simulator/mock backends, `AnyClient` dispatch, outbound rate limiting                                                                                                       | `src/lib.rs`                                                                 | -                                                               |
| `limerick/crates/limerick-setup/`       | LLM local-inference bootstrap: GPU detect, model select, Ollama/vllm process management, install/start/pull/warmup orchestration; re-exported as `limerick_inference::setup`                                      | `src/lib.rs`                                                                 | -                                                               |
| `limerick/crates/limerick-inference/`   | LLM scheduling: request queue + priority lanes, worker, timeout, validation, file logging; delegates transport to `limerick-providers` and setup to `limerick-setup` (re-exported as `limerick_inference::setup`) | `src/lib.rs`                                                                 | [AGENTS.md](../../limerick/crates/limerick-inference/AGENTS.md) |
| `limerick/crates/limerick-input/`       | Player input parsing and command interpretation                                                                                                                                                                   | `src/lib.rs`                                                                 | -                                                               |
| `limerick/crates/limerick-npc/`         | NPC simulation, memory, schedules, tiers, reactions, and autonomous updates                                                                                                                                       | `src/lib.rs`                                                                 | [AGENTS.md](../../limerick/crates/limerick-npc/AGENTS.md)       |
| `limerick/crates/limerick-mod/`         | Content-mod loader: manifest, discovery, runtime data; re-exported as `limerick_core::game_mod`                                                                                                                   | `src/lib.rs`, [README](../../limerick/crates/limerick-mod/README.md)         | -                                                               |
| `limerick/crates/limerick-diagnostics/` | Diagnostics: debug-snapshot builders (`DebugSnapshot`) + bug-report orchestration (GitHub issue / dry-run bundle); re-exported as `limerick_core::debug_snapshot` + `limerick_core::ipc::bug_report`              | `src/lib.rs`, [README](../../limerick/crates/limerick-diagnostics/README.md) | -                                                               |
| `limerick/crates/limerick-editor/`      | Limerick Designer backend: mod browsing, NPC/location editing, validation, deterministic persistence, save inspection; re-exported as `limerick_core::editor`                                                     | `src/lib.rs`, [README](../../limerick/crates/limerick-editor/README.md)      | -                                                               |
| `limerick/crates/limerick-chronicle/`   | On-disk chronicle writers: per-character/player + per-location markdown logs and the JSONL chat transcript; re-exported as `limerick_core::{character_log, location_log, chat_transcript}`                        | `src/lib.rs`, [README](../../limerick/crates/limerick-chronicle/README.md)   | -                                                               |
| `limerick/crates/limerick-palette/`     | Mood and colour palette helpers                                                                                                                                                                                   | `src/lib.rs`                                                                 | -                                                               |
| `limerick/crates/limerick-persistence/` | SQLite saves, branches, snapshots, and user-data path helpers                                                                                                                                                     | `src/lib.rs`                                                                 | -                                                               |
| `limerick/crates/limerick-world/`       | Geography, map graph, weather, and world loading                                                                                                                                                                  | `src/lib.rs`                                                                 | -                                                               |
| `limerick/crates/limerick-types/`       | Shared serde types and cross-crate data contracts                                                                                                                                                                 | `src/lib.rs`                                                                 | -                                                               |
| `limerick/crates/limerick-mcp/`         | MCP server bridging Claude/Codex to a running Limerick backend                                                                                                                                                    | `src/main.rs`, [README](../../limerick/crates/limerick-mcp/README.md)        | -                                                               |
| `limerick/crates/limerick-geo-tool/`    | Geo CLI used by the `/rundale-geo-tool` skill                                                                                                                                                                     | `src/main.rs`                                                                | -                                                               |
| `limerick/crates/limerick-npc-tool/`    | NPC editing and validation CLI                                                                                                                                                                                    | `src/main.rs`                                                                | -                                                               |
| `limerick/crates/limerick-harness/`     | Game quality-control harness: LLM-driven N-turn playtests, gate+axes scoring, findings, SQLite telemetry                                                                                                          | `src/run/runner.rs`, `src/score/`, `src/client/`                             | `limerick/crates/limerick-harness/CLAUDE.md`                    |
| `limerick/crates/limerick-scenario/`    | Versioned YAML scenario runner over the shipping game loop; deterministic inference mocks and machine assertions                                                                                                  | `src/lib.rs`, `src/main.rs`                                                  | [AGENTS.md](../../limerick/crates/limerick-scenario/AGENTS.md)  |

## Local / Generated Paths

| Path                            | Purpose                                                              | Commit policy            |
| ------------------------------- | -------------------------------------------------------------------- | ------------------------ |
| `.proofs/`                      | Per-task proof bundles for rule #10, posted with `just attach-proof` | gitignored; never commit |
| .worktrees/, .claude/worktrees/ | Local agent worktrees and temporary branches                         | local/generated          |
| logs/, saves/                   | Root-level runtime output from local runs                            | local/generated          |
| limerick/logs/, limerick/saves/ | Limerick workspace runtime output and local save branches            | local/generated          |
| limerick/target/                | Cargo build artifacts, coverage, and temp output                     | local/generated          |

## Entry points (binaries)

See [README Ways to run Limerick](../../README.md#ways-to-run-limerick) for the full diagram + table.

- `limerick-engine` - in-process engine binary:
  - `limerick-engine --headless` - stdin/stdout REPL
  - `limerick-engine --script FILE` - deterministic batch driver
  - `limerick-engine` (no flag) - Tauri-launch (when a display is available)
- `limerick-server --port PORT` - Axum HTTP/WS server (separate binary, also exported as library)
- `limerick` (crate `limerick-client`) - thin HTTP shell against a running `limerick-server`. Modes: single-shot `"cmd"`, `--script FILE`, `--json`, no-arg REPL.
- `limerick-tauri` (desktop) - `cargo run -p limerick-tauri -- --mcp-port 3030`
- `limerick-mcp` - MCP bridge for Claude Code/Codex, launched by `limerick/scripts/limerick-mcp-launch.sh` (with a no-build cold shim)
- `limerick-geo-tool`, `limerick-npc-tool` - content-authoring CLIs

## Where to find things

- **Architecture rules:** [`architecture.md`](architecture.md)
- **Build / test commands:** [`build-test.md`](build-test.md)
- **Gotchas (Tokio, SQLite, IPC parity):** [`gotchas.md`](gotchas.md)
- **Harness map (sensors / skills / gates):** [`harness.md`](harness.md)
- **Scaling seam checklist:** [`scaling-rules.md`](scaling-rules.md)
- **Proof-evidence gate:** [`agent-check.md`](agent-check.md)
- **Visual-client and graphics research:** [`../graphics-v2/README.md`](../graphics-v2/README.md)

## Refresh Checklist

When updating this map, compare it against the current tree:

```sh
git ls-files | cut -d/ -f1 | sort -u
find limerick -maxdepth 1 -mindepth 1 -type d | sort
find limerick/crates -maxdepth 1 -mindepth 1 -type d | sort
find mods -maxdepth 1 -mindepth 1 -type d | sort
git ls-files '*AGENTS.md' '*CLAUDE.md'
```
