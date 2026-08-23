# Codebase Map

One-page navigation index for the repository. It lists checked-in top-level
directories, the Parish workspace roots, and local/generated directories agents
are likely to see. Scoped instructions live in `AGENTS.md`; `CLAUDE.md` is a
symlink where present.

## Repository Layout

| Path                                              | Purpose                                                                      | Entry / key file                        | Scope doc                                   |
| ------------------------------------------------- | ---------------------------------------------------------------------------- | --------------------------------------- | ------------------------------------------- |
| `AGENTS.md`, `CLAUDE.md`                          | Repo-wide agent instructions. `CLAUDE.md` is a symlink to `AGENTS.md`        | [AGENTS.md](../../AGENTS.md)            | [AGENTS.md](../../AGENTS.md)                |
| `LEARNINGS.md`                                    | Short-lived gotchas and surprising defaults for future agents                | [LEARNINGS.md](../../LEARNINGS.md)      | -                                           |
| `parish/`                                         | Main Rust workspace and frontend workspace for the Parish engine             | [Cargo.toml](../../parish/Cargo.toml)   | -                                           |
| `parish/crates/`                                  | 24 Rust workspace crates: binaries, composition crate, and leaf logic crates | see [Parish crates](#parish-crates)     | per crate                                   |
| `parish/apps/ui/`                                 | Svelte 5 + TypeScript frontend shared by desktop and web modes               | `src/routes/`, `src/lib/`               | [AGENTS.md](../../parish/apps/ui/AGENTS.md) |
| `parish/testing/`                                 | Asserted scenarios, legacy fixtures, proof scripts, evals, and test data     | `scenarios/`, `fixtures/`, `proofs/`    | [AGENTS.md](../../parish/testing/AGENTS.md) |
| `parish/scripts/`                                 | Check, proof, MCP-backend, screenshot, and release helper scripts            | `*.sh`, `*.py`                          | -                                           |
| `parish/assets/`                                  | Bundled app assets such as fonts                                             | `fonts/`                                | -                                           |
| `parish/dist/`                                    | Runtime distribution helpers and local model/proxy assets                    | `vllm-mlx/`                             | -                                           |
| `parish/docs/`                                    | Parish-local generated docs/screenshots used by the app workspace            | `screenshots/`                          | -                                           |
| `mods/`                                           | Game/content mods, provider mods, and settings mods                          | `mod-list.toml`, provider dirs          | -                                           |
| `mods/rundale/`                                   | Rundale game content: NPCs, world, prompts, palette, and mod metadata        | `mod.toml`                              | [AGENTS.md](../../mods/rundale/AGENTS.md)   |
| `mods/testbed/`                                   | Small settings/content mod for deterministic tests                           | `mod.toml`                              | -                                           |
| `rundale-bench/`                                  | v1 dialogue benchmark, candidate configs, and bench artifacts                | `candidates_*.toml`, `artifacts/`       | -                                           |
| `promptfoo/`                                      | v2 benchmark of record + generated GitHub Pages site (`bench-site/`)         | `leaderboard/`, `bench-site/`           | -                                           |
| `docs/`                                           | Project documentation hub                                                    | [`index.md`](../index.md)               | -                                           |
| `docs/agent/`                                     | Agent-facing engineering docs                                                | [`README.md`](README.md)                | -                                           |
| `docs/graphics-v2/`                               | Visual-client research, art provenance, and reproducible rendering evidence  | [`README.md`](../graphics-v2/README.md) | [AGENTS.md](../graphics-v2/AGENTS.md)       |
| `docs/proofs/`                                    | Ignored local/iCloud proof archives (`local-perf/`, `rundale-bench/`)        | -                                       | -                                           |
| `docs/screenshots/`                               | Checked-in UI screenshot baselines                                           | `*.png`                                 | -                                           |
| `docs/adr/`, `docs/design/`, `docs/plans/`        | Architecture records, design notes, and planning docs                        | `*.md`                                  | -                                           |
| `docs/research/`, `docs/reviews/`, `docs/audits/` | Research notes, review artifacts, and audits                                 | `*.md`                                  | -                                           |
| `deploy/`                                         | Packaging and release artifacts                                              | `Dockerfile`                            | -                                           |
| `scripts/`                                        | Root-level utility scripts outside the Parish workspace                      | `loc_projection.py`                     | -                                           |
| `crates/`                                         | Root-level Rust examples/experiments outside the Parish workspace            | `parish-world/examples/`                | -                                           |
| `.agents/`                                        | Tool-agnostic agent assets and source skills                                 | `skills/`                               | -                                           |
| `.claude/`                                        | Claude Code hooks, commands, agents, and local settings                      | `settings.json`, `hooks/`               | -                                           |
| `.claude-plugin/`                                 | Distributable Rundale plugin manifest                                        | `plugin.json`                           | -                                           |
| `.codex/`                                         | Codex project skill/config assets                                            | `skills/`                               | -                                           |
| `.opencode/`                                      | opencode agents, commands, skills, tools, and plugin config                  | `opencode.jsonc`, `skills/`             | -                                           |
| `.github/`                                        | GitHub workflows, commands, labels, and PR template                          | `workflows/`                            | -                                           |
| `.devcontainer/`                                  | Dev container image and editor setup                                         | `devcontainer.json`                     | -                                           |
| `.vscode/`                                        | Workspace editor tasks, launch configs, and settings                         | `settings.json`, `tasks.json`           | -                                           |

## Parish Crates

The Parish workspace currently has 24 crates under `parish/crates/`.

| Path                                | Purpose                                                                                                                                                                                                     | Entry / key file                                                         | Scope doc                                                   |
| ----------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------ | ----------------------------------------------------------- |
| `parish/crates/parish-engine/`      | Binary `parish-engine`: `--headless`, `--script FILE`, and Tauri-default engine-in-process launch                                                                                                           | `src/main.rs`                                                            | -                                                           |
| `parish/crates/parish-client/`      | Binary `parish`: thin HTTP client for `parish-server` (`POST /api/command`, `GET /api/state`)                                                                                                               | `src/main.rs`, `src/client.rs`                                           | -                                                           |
| `parish/crates/parish-server/`      | Axum HTTP/WebSocket web backend. Library (`run_server`) plus binary (`cargo run -p parish-server -- --port 3001`)                                                                                           | `src/main.rs`, `src/lib.rs`                                              | [AGENTS.md](../../parish/crates/parish-server/AGENTS.md)    |
| `parish/crates/parish-tauri/`       | Desktop app shell and MCP bridge                                                                                                                                                                            | `src/lib.rs`, `src/mcp_bridge.rs`                                        | [AGENTS.md](../../parish/crates/parish-tauri/AGENTS.md)     |
| `parish/crates/parish-core/`        | Backend-agnostic composition crate and shared orchestration                                                                                                                                                 | `src/lib.rs`                                                             | [AGENTS.md](../../parish/crates/parish-core/AGENTS.md)      |
| `parish/crates/parish-config/`      | Game, provider, runtime, and flag configuration                                                                                                                                                             | `src/lib.rs`                                                             | -                                                           |
| `parish/crates/parish-providers/`   | LLM transport: provider HTTP clients, simulator/mock backends, `AnyClient` dispatch, outbound rate limiting                                                                                                 | `src/lib.rs`                                                             | -                                                           |
| `parish/crates/parish-setup/`       | LLM local-inference bootstrap: GPU detect, model select, Ollama/vllm process management, install/start/pull/warmup orchestration; re-exported as `parish_inference::setup`                                  | `src/lib.rs`                                                             | -                                                           |
| `parish/crates/parish-inference/`   | LLM scheduling: request queue + priority lanes, worker, timeout, validation, file logging; delegates transport to `parish-providers` and setup to `parish-setup` (re-exported as `parish_inference::setup`) | `src/lib.rs`                                                             | [AGENTS.md](../../parish/crates/parish-inference/AGENTS.md) |
| `parish/crates/parish-input/`       | Player input parsing and command interpretation                                                                                                                                                             | `src/lib.rs`                                                             | -                                                           |
| `parish/crates/parish-npc/`         | NPC simulation, memory, schedules, tiers, reactions, and autonomous updates                                                                                                                                 | `src/lib.rs`                                                             | [AGENTS.md](../../parish/crates/parish-npc/AGENTS.md)       |
| `parish/crates/parish-mod/`         | Content-mod loader: manifest, discovery, runtime data; re-exported as `parish_core::game_mod`                                                                                                               | `src/lib.rs`, [README](../../parish/crates/parish-mod/README.md)         | -                                                           |
| `parish/crates/parish-diagnostics/` | Diagnostics: debug-snapshot builders (`DebugSnapshot`) + bug-report orchestration (GitHub issue / dry-run bundle); re-exported as `parish_core::debug_snapshot` + `parish_core::ipc::bug_report`            | `src/lib.rs`, [README](../../parish/crates/parish-diagnostics/README.md) | -                                                           |
| `parish/crates/parish-editor/`      | Parish Designer backend: mod browsing, NPC/location editing, validation, deterministic persistence, save inspection; re-exported as `parish_core::editor`                                                   | `src/lib.rs`, [README](../../parish/crates/parish-editor/README.md)      | -                                                           |
| `parish/crates/parish-chronicle/`   | On-disk chronicle writers: per-character/player + per-location markdown logs and the JSONL chat transcript; re-exported as `parish_core::{character_log, location_log, chat_transcript}`                    | `src/lib.rs`, [README](../../parish/crates/parish-chronicle/README.md)   | -                                                           |
| `parish/crates/parish-palette/`     | Mood and colour palette helpers                                                                                                                                                                             | `src/lib.rs`                                                             | -                                                           |
| `parish/crates/parish-persistence/` | SQLite saves, branches, snapshots, and user-data path helpers                                                                                                                                               | `src/lib.rs`                                                             | -                                                           |
| `parish/crates/parish-world/`       | Geography, map graph, weather, and world loading                                                                                                                                                            | `src/lib.rs`                                                             | -                                                           |
| `parish/crates/parish-types/`       | Shared serde types and cross-crate data contracts                                                                                                                                                           | `src/lib.rs`                                                             | -                                                           |
| `parish/crates/parish-mcp/`         | MCP server bridging Claude/Codex to a running Parish backend                                                                                                                                                | `src/main.rs`, [README](../../parish/crates/parish-mcp/README.md)        | -                                                           |
| `parish/crates/parish-geo-tool/`    | Geo CLI used by the `/rundale-geo-tool` skill                                                                                                                                                               | `src/main.rs`                                                            | -                                                           |
| `parish/crates/parish-npc-tool/`    | NPC editing and validation CLI                                                                                                                                                                              | `src/main.rs`                                                            | -                                                           |
| `parish/crates/parish-harness/`     | Game quality-control harness: LLM-driven N-turn playtests, gate+axes scoring, findings, SQLite telemetry                                                                                                    | `src/run/runner.rs`, `src/score/`, `src/client/`                         | `parish/crates/parish-harness/CLAUDE.md`                    |
| `parish/crates/parish-scenario/`    | Versioned YAML scenario runner over the shipping game loop; deterministic inference mocks and machine assertions                                                                                            | `src/lib.rs`, `src/main.rs`                                              | [AGENTS.md](../../parish/crates/parish-scenario/AGENTS.md)  |

## Local / Generated Paths

| Path                            | Purpose                                                              | Commit policy            |
| ------------------------------- | -------------------------------------------------------------------- | ------------------------ |
| `.proofs/`                      | Per-task proof bundles for rule #10, posted with `just attach-proof` | gitignored; never commit |
| .worktrees/, .claude/worktrees/ | Local agent worktrees and temporary branches                         | local/generated          |
| logs/, saves/                   | Root-level runtime output from local runs                            | local/generated          |
| parish/logs/, parish/saves/     | Parish workspace runtime output and local save branches              | local/generated          |
| parish/target/                  | Cargo build artifacts, coverage, and temp output                     | local/generated          |

## Entry points (binaries)

See [README Ways to run Parish](../../README.md#ways-to-run-parish) for the full diagram + table.

- `parish-engine` - in-process engine binary:
  - `parish-engine --headless` - stdin/stdout REPL
  - `parish-engine --script FILE` - deterministic batch driver
  - `parish-engine` (no flag) - Tauri-launch (when a display is available)
- `parish-server --port PORT` - Axum HTTP/WS server (separate binary, also exported as library)
- `parish` (crate `parish-client`) - thin HTTP shell against a running `parish-server`. Modes: single-shot `"cmd"`, `--script FILE`, `--json`, no-arg REPL.
- `parish-tauri` (desktop) - `cargo run -p parish-tauri -- --mcp-port 3030`
- `parish-mcp` - MCP bridge for Claude Code/Codex, launched by `parish/scripts/parish-mcp-launch.sh` (with a no-build cold shim)
- `parish-geo-tool`, `parish-npc-tool` - content-authoring CLIs

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
find parish -maxdepth 1 -mindepth 1 -type d | sort
find parish/crates -maxdepth 1 -mindepth 1 -type d | sort
find mods -maxdepth 1 -mindepth 1 -type d | sort
git ls-files '*AGENTS.md' '*CLAUDE.md'
```
