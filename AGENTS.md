# Repository Guidelines — Rundale on the Parish Engine

`AGENTS.md` is the source of truth for repo guidelines. `CLAUDE.md` is a symlink to it, so any edit here is automatically visible to Claude Code as well.

**Before anything else, skim [LEARNINGS.md](LEARNINGS.md)** — brief
bullet list of gotchas and surprising defaults that aren't obvious from
reading code or git history. Append a bullet whenever you discover
something a future agent would benefit from knowing. The
`Stop--learnings-reminder` hook nudges you to revisit it on non-trivial
sessions.

Start with the detailed agent docs in [docs/agent/README.md](docs/agent/README.md):

- [build-test.md](docs/agent/build-test.md) — cargo, harness, frontend, web, and Tauri commands
- [architecture.md](docs/agent/architecture.md) — workspace layout and module ownership
- [code-style.md](docs/agent/code-style.md) — Rust + Svelte conventions
- [gotchas.md](docs/agent/gotchas.md) — Tokio, SQLite, Ollama, mode parity pitfalls
- [git-workflow.md](docs/agent/git-workflow.md) — commits, tests, and PR standards
- [agent-check.md](docs/agent/agent-check.md) — proof evidence and judge verdict gate
- [skills.md](docs/agent/skills.md) — `/check`, `/parish-engine`, `/task-start`, `/backlog`, etc.
- [harness.md](docs/agent/harness.md) — one-page map of every sensor, skill, and gate (start here when something fails)
- [codebase-map.md](docs/agent/codebase-map.md) — top-level directory index with per-area `CLAUDE.md` pointers

**Rundale** is the game (Irish living world, 1820). **Parish** is the engine (Rust workspace + frontends).

## Current project state (quick map)

- Rust workspace: **17 crates** under `parish/crates/` — see [docs/agent/architecture.md](docs/agent/architecture.md) for the full table.
  - Binaries: `parish-engine` (in-process engine — `--headless` / `--script FILE` / Tauri-launch), `parish-server` (Axum HTTP/WS server, library + binary), `parish-tauri` (desktop), `parish-client` (binary `parish`, thin HTTP client — see [Ways to run Parish](README.md#ways-to-run-parish)), `parish-mcp`, `parish-geo-tool`, `parish-npc-tool`.
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

1. **Module ownership (enforced):** Shared logic belongs in a leaf crate (`parish-config`, `parish-inference`, `parish-input`, `parish-npc`, `parish-palette`, `parish-persistence`, `parish-world`, `parish-types`). `parish-core` composes them. Do not duplicate leaf-crate logic in `parish/crates/parish-engine/src/`. Orphaned source files (present on disk but not declared as `mod`) are also rejected.
2. **Mode parity (partially enforced):** Tauri, headless CLI, and web server must share behavior. The architecture-fitness test forbids backend-agnostic crates from depending on `tauri` / `axum` / `tower*` / `wry` / `tao`, so runtime-specific code can't leak into shared logic. Wiring parity (every IPC handler called from every entry point) is still convention.
3. **Tests with behavior changes:** Add/adjust tests for every behavior change.
4. **Gameplay proof:** For gameplay features, run `/parish-engine prove <feature>` (unit tests alone are not sufficient).
5. **No unexplained `#[allow]`:** Only with explicit justification.
6. **Feature flags for new engine/gameplay features:** Gate with `config.flags.is_enabled("feature-name")`, default-on, and document in PR.
7. **Keep README.md up to date.** Especially the feature list, repository structure and credits. Run `just notices` to update third party notices when dependencies are changed.
8. **Investigate with Five Whys.** When diagnosing a bug, regression, or unexpected behavior, run the `/five-whys` skill (or apply the method) to reach the root cause before patching.
9. **Resolve runtime paths from explicit config, not the cwd.** Saves dir, mods dir, data dir, and similar runtime paths must be resolved once at startup (env var or active-mod identity) and stored on `AppState` / `GlobalState`. Never call `current_dir()`, parent-walks, or marker-file searches from request handlers or per-call helpers — packaged builds, daemonised servers, and `/tmp` working directories all break that assumption (#771). Saves + tile cache live under the platform user-data root: `~/Library/Application Support/<app>` (macOS), `$XDG_DATA_HOME/<app>` (Linux), `%APPDATA%\<app>` (Windows). `app` comes from the active mod's `ModMeta::app_name()` (`save_root` field, falling back to `name`; engine fallback is `paths::DEFAULT_APP_NAME`). Use `parish_persistence::picker::resolve_project_saves_dir(app_name)` for saves and `parish_persistence::paths::resolve_user_data_dir(app_name)` for any other per-user data — both honour env-var overrides (`PARISH_SAVES_DIR` / `PARISH_TILE_CACHE_DIR` at the leaf; `PARISH_USER_DATA_DIR` at the root).
10. **Proof evidence for proof-relevant PRs (enforced):** Runtime, UI, gameplay, CI, harness, and agent-instruction changes that include a code change must ship a proof bundle attached to the PR with a gameplay transcript, screenshot, or gif plus an independent judge verdict. Bundles live locally in `.proofs/<task-id>/` (gitignored) and are carried in the PR **body** as a structured fenced block — they are **never committed**. `just agent-check` (local mode, walks `.proofs/`) and CI (`--source=pr <num>`, reads the PR body and comments) reject missing proof or recorded debt. The two long-lived archives under `docs/proofs/local-perf/` and `docs/proofs/rundale-bench/` are bench artifacts (eval-dialogue reports, ELO logs) and are exempt from this gate. PRs that touch no source/runtime paths are exempt: pure documentation (e.g. AGENTS.md, README.md, `docs/**`, any `*.md`/`*.txt`), CI-only (`.github/**`), agent-instruction-only (`.agents/**`, `.claude/**`), check-tooling-only (`parish/scripts/**`), and build-config-only (`justfile`) edits all skip the gate when no code change accompanies them. Dependabot PRs are also exempt at the CI layer — automated dependency bumps have no useful signal to prove. Create the PR with the bundle already in the body so the gate is green on the **first** run — `gh pr create --body-file <(printf '%s\n' "$description" | bash parish/scripts/compose-proof-body.sh <task-id>)`. `just attach-proof <task-id>` (re-)injects the bundle into the body of an existing PR (idempotent); use it after fixing a bundle. The body is read on the `pull_request.opened` run, so this no longer races a post-creation comment (#1177). The legacy `--as-comment` mode still works but loses that race.

    **Live-proof tier (enforced).** When the diff touches a runtime-shipping path — `parish-tauri/**`, `parish-server/**`, `parish-engine/**`, `parish-core/src/{game_loop,game_session,ipc}/**`, `parish-inference/src/{setup,client}.rs`, `parish-npc/src/{ticks,manager,reactions,autonomous}/**`, `parish-world/**`, `parish-input/**`, `parish/apps/ui/src/**`, `mods/**` — unit tests alone are not sufficient. The change must be exercised in a real process (Tauri, server, CLI, or browser) and the bundle's `evidence.md` header must declare `Evidence type: live gameplay transcript`, **or** the bundle must include a screenshot (`.png` / `.jpg` / `.jpeg`) or gif (`.gif`). The word "live" is the author affirmation that the run actually happened; analysis-only writeups failing this header are rejected by `just agent-check`. Accepted live signals: `mcp__parish__*`, `mcp__claude-in-chrome__*`, the `/parish-engine` skill (its `prove` / `play` / `demo` / `browser` modes), or a Bash invocation of `just demo` / `just play` / `just run` / `just run-headless` / `just web` / `cargo tauri dev` / `cargo run -p parish-{engine,tauri,server,client}`. The Stop hook (`.claude/hooks/Stop--proof-required.sh`) blocks session-end with the same matrix.

11. **Scaling guardrails:** Any PR that touches `AppState`, session persistence, real-time push, inference calls, identity lookups, mod loading, or request-ID tracing must be reviewed against the seam checklist in [docs/agent/scaling-rules.md](docs/agent/scaling-rules.md). Each rule names the seam file it protects.
12. **Cross-runtime orchestration belongs in `parish-core`:** Any game-loop, IPC, or session handler shared by the server, Tauri, and CLI entry points — including its supporting constants, payload structs, and helper functions — must be defined once in a backend-agnostic crate and parameterized over runtime-specific concerns via traits (e.g. `EventEmitter`), with each entry-point crate (`parish-server`, `parish-tauri`, `parish-engine` — the latter lives in `parish/crates/parish-engine/` on disk) limited to thin wiring that adapts its emitter and I/O to the shared core. Copy-pasting an orchestration body, constant, or IPC payload struct into a second entry-point crate is forbidden, because the divergence is invisible at review time and silently produces security drift (#687, #696).
13. **Acceptance-criteria-first (partially enforced):** Every implementation task that changes code must begin by writing `.proofs/<task-id>/acceptance-criteria.md` listing observable criteria **before** writing any code. Use `/task-start <task-id>` to generate this artifact and the companion verification fixture at `parish/testing/fixtures/play_<task-id>.txt`. The proof evidence must include a live game log (from `cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script ...`, `parish --script ...` (live server), `just run-headless`, `mcp__parish__*`, or equivalent) with a section that maps each criterion to the output line(s) that prove it. The `judge.md` must explicitly verify every criterion and include the line `Acceptance criteria: met`. `just agent-check` and the Stop hook enforce that an `acceptance-criteria.md` was written whenever a proof bundle is produced. The bundle itself stays in `.proofs/<task-id>/` (gitignored); use `just attach-proof <task-id>` to post it to the PR. The sequential order is non-negotiable: write AC → get approval → implement → run game → capture log → write evidence → judge against AC → attach to PR.
14. **Validate artifact content, not just the envelope.** A tool/handler that returns a produced artifact (screenshot, export, render, generated file) must verify the artifact carries real content before reporting success — never treat "produced syntactically valid, nonzero-length bytes" as proof. A blank/degenerate result must surface an `Err`, not a success, so downstream consumers (e.g. `parish_file_bug` bundling a screenshot) can't ship an empty artifact. Enforced for screenshot capture by `reject_blank_capture` in `parish/crates/parish-tauri/src/commands/screenshot.rs` (runs at the shared chokepoint for both the native `xcap` and `html-to-image` paths); apply the same shape to any new artifact-returning path (#1301).

## Standard commands

```sh
just build         # cargo build (default member parish-engine)
just run-client    # cargo run -p parish-client (thin HTTP client → running server)
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

| Tool                       | Effect                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| -------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `parish_world_snapshot`    | Read clock, player location, weather, recent log.                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `parish_map`               | Read the location graph plus the player's position.                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `parish_npcs_here`         | List NPCs co-located with the player.                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `parish_engine_state`      | Read the canonical deterministic engine state for QA validation — `active_scene`, `clock`, `weather`, `player`, `npcs`, `grapevine`. Assert the UI against this after each interaction to detect UI-vs-engine drift (#1331).                                                                                                                                                                                                                                                                             |
| `parish_save_state`        | Read save-file / branch metadata.                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `parish_submit_input`      | Send player input — movement, action, dialogue, system commands. Optional `addressed_to` array scopes dialogue.                                                                                                                                                                                                                                                                                                                                                                                          |
| `parish_new_game`          | Start a fresh game on a new save branch.                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `parish_save_game`         | Save the current branch.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `parish_load_branch`       | Load a branch by integer id.                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `parish_setup_status`      | Reads first-run setup state: `{implemented, complete, provider, model, base_url, has_api_key, has_env_key}`.                                                                                                                                                                                                                                                                                                                                                                                             |
| `parish_setup_byok`        | Submits a BYOK provider config (`provider`, `api_key`, optional `base_url`/`model`). Persists to keychain + `parish.toml`, rebuilds the inference worker, emits `setup-done`.                                                                                                                                                                                                                                                                                                                            |
| `parish_latest_screenshot` | Read metadata for the most recent player-triggered screenshot (`path`, `taken_at`, `size_bytes`). Capture is initiated by pressing F2 in the live desktop window.                                                                                                                                                                                                                                                                                                                                        |
| `parish_file_bug`          | File a bug report (`title`, optional `description`/`context`). Bundles a live screenshot + recent logs + game state into a GitHub issue on the configured repo (`dmooney/rundale` by default) and returns the issue URL. Auto-appends a "black box" diagnostic payload — raw LLM prompt/response history, the `get_engine_state` snapshot, and the last raw user intent (#1331). In dry-run / no-token mode writes the composed report to disk (`created:false`, `bundle_path` set). For auto-QA agents. |
| `tauri_invoke`             | Generic escape hatch — call any backend command (e.g. `editor_*`, `get_debug_snapshot`) by name.                                                                                                                                                                                                                                                                                                                                                                                                         |

The MCP server is a _bridge_: it speaks HTTP to a running Parish backend on
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

## Driving Parish via the `parish` CLI client

The `parish` binary (`parish-client` crate) is a thin synchronous HTTP client for a
**running** Parish server. Calls `POST /api/command` and returns the full response in one
round-trip — no WebSocket, no polling.

```sh
# Start the server first:
bash parish/scripts/parish-mcp-backend.sh start     # port 3030
# or: just web 3001                                  # port 3001

# Drive it:
parish [--server http://localhost:3001] "look"       # single-shot
parish --script testing/fixtures/test_walkthrough.txt  # batch
parish                                               # interactive REPL
parish --json "go to the church" | jq .kind         # raw JSON

# PARISH_SERVER env var sets the default URL:
PARISH_SERVER=http://localhost:3001 parish "look"
```

Use `parish --script` for proof transcripts requiring real NPC inference.
Use `just run-headless --script` for deterministic, fast harness-level testing.

## Commit and PR expectations

- Conventional commits: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`.
- One logical change per commit.
- PRs should explain behavior changes, link issues, list commands run, and include screenshots / updated Playwright baselines for visible UI changes.
