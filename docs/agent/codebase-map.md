# Codebase Map

One-page index of every top-level directory. Use this when navigating an unfamiliar area before diving in. Per-directory `CLAUDE.md` files give scoped commands and gotchas.

## Top-level layout

| Path | Purpose | Entry / key file | Scope doc |
|---|---|---|---|
| `parish/crates/parish-cli/` | Headless `parish` CLI binary | `src/main.rs` | — |
| `parish/crates/parish-server/` | Axum HTTP/WebSocket web backend | `src/main.rs` | [CLAUDE.md](../../parish/crates/parish-server/CLAUDE.md) |
| `parish/crates/parish-tauri/` | Desktop app + MCP bridge | `src/lib.rs`, `src/mcp_bridge.rs` | [CLAUDE.md](../../parish/crates/parish-tauri/CLAUDE.md) |
| `parish/crates/parish-core/` | Backend-agnostic composition crate | `src/lib.rs` | [CLAUDE.md](../../parish/crates/parish-core/CLAUDE.md) |
| `parish/crates/parish-config/` | Game + provider config | `src/lib.rs` | — |
| `parish/crates/parish-inference/` | LLM clients + queue | `src/lib.rs` | [CLAUDE.md](../../parish/crates/parish-inference/CLAUDE.md) |
| `parish/crates/parish-input/` | Player input parsing | `src/lib.rs` | — |
| `parish/crates/parish-npc/` | NPC sim + memory + tiers | `src/lib.rs` | [CLAUDE.md](../../parish/crates/parish-npc/CLAUDE.md) |
| `parish/crates/parish-palette/` | Mood/colour palette | `src/lib.rs` | — |
| `parish/crates/parish-persistence/` | SQLite saves + branches | `src/lib.rs` | — |
| `parish/crates/parish-world/` | Geography + map graph | `src/lib.rs` | — |
| `parish/crates/parish-types/` | Shared serde types | `src/lib.rs` | — |
| `parish/crates/parish-mcp/` | MCP server bridging Claude → Parish | `src/main.rs`, [README](../../parish/crates/parish-mcp/README.md) | — |
| `parish/crates/parish-geo-tool/` | Geo CLI (`/rundale-geo-tool` skill) | `src/main.rs` | — |
| `parish/crates/parish-npc-tool/` | NPC editing CLI | `src/main.rs` | — |
| `parish/apps/ui/` | Svelte 5 + TS frontend (one for all modes) | `src/routes/`, `src/lib/` | [CLAUDE.md](../../parish/apps/ui/CLAUDE.md) |
| `parish/testing/fixtures/` | Harness scripts | — | [CLAUDE.md](../../parish/testing/CLAUDE.md) |
| `parish/testing/evals/` | Rubric configs | — | (same) |
| `parish/testing/rundale-bench/` | ELO dialogue benchmark | — | (same) |
| `parish/scripts/` | Check/CI helpers (shellcheck-clean) | — | — |
| `mods/rundale/` | Game content (NPCs, world, prompts) | `mod.toml` | [CLAUDE.md](../../mods/rundale/CLAUDE.md) |
| `docs/` | Project documentation hub | [`index.md`](../index.md) | — |
| `docs/agent/` | Agent-facing engineering docs | [`README.md`](README.md) | — |
| `docs/proofs/` | Proof bundles (rule #10) | — | — |
| `docs/screenshots/` | UI baselines | — | — |
| `deploy/` | Packaging + release artifacts | — | — |
| `.claude/` | Claude Code config (skills, hooks, settings) | `settings.json` | — |
| `.claude-plugin/` | Distributable Rundale plugin manifest | `plugin.json` | — |
| `.agents/` | Tool-agnostic agent assets (skills source) | `skills/` | — |

## Entry points (binaries)

- `parish` — headless CLI (`parish-cli`)
- `parish web` / `parish-server` — Axum web server
- `parish-tauri` (desktop) — `cargo run -p parish-tauri -- --mcp-port 3030`
- `parish-mcp` — MCP bridge for Claude Code (auto-built by `SessionStart--build-mcp.sh`)
- `parish-geo-tool`, `parish-npc-tool` — content-authoring CLIs

## Where to find things

- **Architecture rules:** [`architecture.md`](architecture.md)
- **Build / test commands:** [`build-test.md`](build-test.md)
- **Gotchas (Tokio, SQLite, IPC parity):** [`gotchas.md`](gotchas.md)
- **Harness map (sensors / skills / gates):** [`harness.md`](harness.md)
- **Scaling seam checklist:** [`scaling-rules.md`](scaling-rules.md)
- **Proof-evidence gate:** [`agent-check.md`](agent-check.md)
