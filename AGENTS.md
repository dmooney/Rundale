# Repository Guidelines — Rundale on the Parish Engine

`AGENTS.md` is the source of truth for repo guidelines. `CLAUDE.md` is a symlink to it, so any edit here is automatically visible to Claude Code as well.

Start with the detailed agent docs in [docs/agent/README.md](docs/agent/README.md):

- [build-test.md](docs/agent/build-test.md) — cargo, harness, frontend, web, and Tauri commands
- [architecture.md](docs/agent/architecture.md) — workspace layout and module ownership
- [code-style.md](docs/agent/code-style.md) — Rust + Svelte conventions
- [gotchas.md](docs/agent/gotchas.md) — Tokio, SQLite, Ollama, mode parity pitfalls
- [git-workflow.md](docs/agent/git-workflow.md) — commits, tests, and PR standards
- [agent-check.md](docs/agent/agent-check.md) — proof evidence and judge verdict gate
- [skills.md](docs/agent/skills.md) — `/check`, `/verify`, `/prove`, `/play`, etc.
- [harness.md](docs/agent/harness.md) — one-page map of every sensor, skill, and gate (start here when something fails)

**Rundale** is the game (Irish living world, 1820). **Parish** is the engine (Rust workspace + frontends).

## Current project state (quick map)

- Rust workspace: **14 crates** under `parish/crates/` — see [docs/agent/architecture.md](docs/agent/architecture.md) for the full table.
  - Binaries: `parish-cli` (CLI/headless `parish`), `parish-server` (Axum web), `parish-tauri` (desktop), `parish-geo-tool`, `parish-npc-tool`.
  - Composition: `parish-core` re-exports the leaf crates under stable namespaces.
  - Leaf logic crates: `parish-config`, `parish-inference`, `parish-input`, `parish-npc`, `parish-palette`, `parish-persistence`, `parish-world`, `parish-types`.
  - These crates make up the **Parish** game engine.
- Frontend: `parish/apps/ui/` (Svelte 5 + TypeScript)
- Rundale game content: `mods/rundale/`
- Test fixtures: `parish/testing/fixtures/`
- Deploy artifacts: `deploy/`
- Documentation hub: `docs/index.md`

## Non-negotiable engineering rules

Rules marked **(enforced)** are checked mechanically by `cargo test` / CI — see `parish/crates/parish-core/tests/architecture_fitness.rs`. The rest are still convention.

1. **Module ownership (enforced):** Shared logic belongs in a leaf crate (`parish-config`, `parish-inference`, `parish-input`, `parish-npc`, `parish-palette`, `parish-persistence`, `parish-world`, `parish-types`). `parish-core` composes them. Do not duplicate leaf-crate logic in `parish/crates/parish-cli/src/`. Orphaned source files (present on disk but not declared as `mod`) are also rejected.
2. **Mode parity (partially enforced):** Tauri, headless CLI, and web server must share behavior. The architecture-fitness test forbids backend-agnostic crates from depending on `tauri` / `axum` / `tower*` / `wry` / `tao`, so runtime-specific code can't leak into shared logic. Wiring parity (every IPC handler called from every entry point) is still convention.
3. **Tests with behavior changes:** Add/adjust tests for every behavior change.
4. **Gameplay proof:** For gameplay features, run `/prove <feature>` (unit tests alone are not sufficient).
5. **No unexplained `#[allow]`:** Only with explicit justification.
6. **Feature flags for new engine/gameplay features:** Gate with `config.flags.is_enabled("feature-name")`, default-on, and document in PR.
7. **Keep README.md up to date.** Especially the feature list, repository structure and credits. Run `just notices` to update third party notices when dependencies are changed.
8. **Investigate with Five Whys.** When diagnosing a bug, regression, or unexpected behavior, run the `/five-whys` skill (or apply the method) to reach the root cause before patching.
9. **Resolve runtime paths from explicit config, not the cwd.** Saves dir, mods dir, data dir, and similar runtime paths must be resolved once at startup (env var, CLI flag, or project-marker probe) and stored on `AppState` / `GlobalState`. Never call `current_dir()`, parent-walks, or marker-file searches from request handlers or per-call helpers — packaged builds, daemonised servers, and `/tmp` working directories all break that assumption (#771). Use `parish_persistence::picker::resolve_project_saves_dir` rather than re-rolling the walk.
10. **Proof evidence for proof-relevant PRs (enforced):** Runtime, UI, gameplay, CI, harness, and agent-instruction changes that include a code change must ship a changed proof bundle under `docs/proofs/` with a gameplay transcript, screenshot, or gif plus an independent judge verdict in `judge.md`. `just agent-check` and CI reject missing proof or recorded debt. PRs that touch no source/runtime paths are exempt: pure documentation (e.g. AGENTS.md, README.md, `docs/**`, any `*.md`/`*.txt`), CI-only (`.github/**`), agent-instruction-only (`.agents/**`, `.claude/**`), check-tooling-only (`parish/scripts/**`), and build-config-only (`justfile`) edits all skip the gate when no code change accompanies them. Dependabot PRs are also exempt at the CI layer — automated dependency bumps have no useful signal to prove.
11. **Scaling guardrails:** Any PR that touches `AppState`, session persistence, real-time push, inference calls, identity lookups, mod loading, or request-ID tracing must be reviewed against the seam checklist in [docs/agent/scaling-rules.md](docs/agent/scaling-rules.md). Each rule names the seam file it protects.
12. **Cross-runtime orchestration belongs in `parish-core`:** Any game-loop, IPC, or session handler shared by the server, Tauri, and CLI entry points — including its supporting constants, payload structs, and helper functions — must be defined once in a backend-agnostic crate and parameterized over runtime-specific concerns via traits (e.g. `EventEmitter`), with each entry-point crate (`parish-server`, `parish-tauri`, `parish-cli`) limited to thin wiring that adapts its emitter and I/O to the shared core. Copy-pasting an orchestration body, constant, or IPC payload struct into a second entry-point crate is forbidden, because the divergence is invisible at review time and silently produces security drift (#687, #696).

## Standard commands

```sh
just build         # cargo build (default member parish-cli)
just run           # cargo tauri dev
just run-headless
just check         # fmt + clippy + tests
just agent-check   # proof evidence + judge verdict gate
just verify        # check + harness walkthrough

just ui-test       # frontend unit tests
just ui-e2e        # Playwright end-to-end tests
just screenshots   # regenerate docs/screenshots/*.png
```

## Driving Parish via MCP (`parish-mcp`)

`.mcp.json` at the repo root registers `parish-mcp` as a project-level MCP
server. When you start a Claude Code session here, the tools below are
available as `mcp__parish__*`:

| Tool | Effect |
| --- | --- |
| `parish_world_snapshot` | Read clock, player location, weather, recent log. |
| `parish_map` | Read the location graph plus the player's position. |
| `parish_npcs_here` | List NPCs co-located with the player. |
| `parish_save_state` | Read save-file / branch metadata. |
| `parish_submit_input` | Send player input — movement, action, dialogue, system commands. Optional `addressed_to` array scopes dialogue. |
| `parish_new_game` | Start a fresh game on a new save branch. |
| `parish_save_game` | Save the current branch. |
| `parish_load_branch` | Load a branch by integer id. |
| `parish_setup_status` | **Stub.** Reads first-run setup state. Backend returns `{"stub": true, ...}` until the setup-UI branch lands. |
| `parish_setup_byok` | **Stub.** Submits a BYOK provider config (`provider`, `api_key`, optional `base_url`/`model`). Same stub envelope. |
| `tauri_invoke` | Generic escape hatch — call any backend command (e.g. `editor_*`, `get_debug_snapshot`) by name. |

The MCP server is a *bridge*: it speaks HTTP to a running Parish backend on
`127.0.0.1:3030`. **Before using any `mcp__parish__*` tool**, ensure a
backend is up:

```sh
# Headless web server (works in any sandbox; recommended in CI / on the web):
bash parish/scripts/parish-mcp-backend.sh start    # spawn + wait for /api/health
bash parish/scripts/parish-mcp-backend.sh status   # report pid + health
bash parish/scripts/parish-mcp-backend.sh stop     # graceful shutdown

# Desktop, when a display is available — drives the live window:
cargo run -p parish-tauri -- --mcp-port 3030
```

If a tool call returns an MCP `isError: true` with `transport error: ...`,
the backend isn't running — call `parish-mcp-backend.sh start` first.

For deeper context on the bridge implementation see
[parish/crates/parish-mcp/README.md](parish/crates/parish-mcp/README.md)
and [parish/crates/parish-tauri/src/mcp_bridge.rs](parish/crates/parish-tauri/src/mcp_bridge.rs).

## Commit and PR expectations

- Conventional commits: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`.
- One logical change per commit.
- PRs should explain behavior changes, link issues, list commands run, and include screenshots / updated Playwright baselines for visible UI changes.
