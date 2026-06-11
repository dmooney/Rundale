# Architecture & Layout

See [docs/design/overview.md](../design/overview.md) for the full architecture and [docs/index.md](../index.md) for all documentation.

**Rundale** is the Irish living world game. **Parish** is the Rust engine it runs on. The repository is a **Cargo workspace** — all engine crates live under `parish/crates/`, the game content lives under `mods/rundale/`, frontends under `parish/apps/`, test fixtures under `parish/testing/`, and deploy artifacts under `deploy/`.

## Workspace crates

The workspace has **20 member crates** (see `parish/Cargo.toml`). Shared game logic is split across focused leaf crates; `parish-core` is a thin composition layer that re-exports them under stable names used by the binaries and frontends.

| Crate                | Role                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| -------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `parish-core`        | Composition crate: re-exports `parish-config`, `parish-editor`, `parish-inference`, `parish-input`, `parish-mod`, `parish-npc`, `parish-palette`, `parish-persistence`, `parish-world`, and `parish-types` under `crate::{config, editor, inference, input, game_mod, npc, palette, persistence, world, error, dice}`. Also owns the IPC layer (`ipc/`), game session wiring (`game_session`), and the shared `prompts/` + `debug_snapshot` modules. The content-mod loader was extracted to `parish-mod` (re-exported as `crate::game_mod`) and the Parish Designer backend to `parish-editor` (re-exported as `crate::editor`). |
| `parish-engine`      | In-process engine entry point (`cargo run -p parish-engine`). Modes: `--headless` (stdin/stdout REPL), `--script FILE` (batch fixture driver), no flag (Tauri-launch). Owns `main.rs` (clap CLI + mode routing), `headless.rs`, `testing.rs` (`GameTestHarness` + `--script` mode), `app.rs`, `debug.rs`, and a CLI-override `config.rs`. Re-exports `parish_core` modules via `pub use parish_core::*`.                                                                                                                                                                                                                          |
| `parish-server`      | Axum web backend (no Tauri dep). Library export `run_server` plus its own `main.rs` so the server boots directly via `cargo run -p parish-server -- --port 3001`. Modules: `lib.rs` (`run_server`, tick loops), `main.rs` (clap + tracing), `state.rs`, `routes.rs`, `ws.rs`, `sync_routes.rs` (synchronous `POST /api/command` + `GET /api/state` for thin clients), `sync_types.rs`, `drain.rs`, `auth.rs`, `cf_auth.rs`, `middleware.rs`, `session.rs`, `editor_routes.rs`.                                                                                                                                                    |
| `parish-client`      | Thin HTTP client (binary `parish`). No engine in-process — calls `POST /api/command` / `GET /api/state` on a running `parish-server`. Modes: `parish "<cmd>"` single-shot, `--script FILE`, `--json`, no-arg REPL. Persists the `parish_sid` cookie between runs. See [README §Ways to run Parish](../../README.md#ways-to-run-parish).                                                                                                                                                                                                                                                                                           |
| `parish-tauri`       | Tauri 2 desktop backend. `tauri.conf.json` → `frontendDist: ../../parish/apps/ui/dist`. Sources: `lib.rs` (AppState + run), `main.rs`, `commands.rs`, `editor_commands.rs`, `events.rs`.                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `parish-mcp`         | MCP server bridge for AI agents (binary `parish-mcp`, registered in `.mcp.json`). Speaks HTTP to a running Parish backend on `127.0.0.1:3030` and exposes the `mcp__parish__*` tools (world snapshot, input, saves, setup, bug filing). Start a backend with `bash parish/scripts/parish-mcp-backend.sh start`. See [parish/crates/parish-mcp/README.md](../../parish/crates/parish-mcp/README.md).                                                                                                                                                                                                                               |
| `parish-config`      | Engine configuration: TOML + env + CLI overrides, feature flags, provider selection. `engine.rs`, `flags.rs`, `provider.rs`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `parish-inference`   | LLM **scheduling** half: queue + priority lanes (`queue.rs`), worker (`worker.rs`), timeout/submit (`timeout.rs`), provider validation/auth probes (`validate.rs`), file logging (`file_log.rs`, `logs.rs`), Ollama/vllm bootstrap (`setup/`), HF model download (`hf_downloader.rs`), Ollama REST client (`client.rs`). Delegates HTTP transport + rate limiting to `parish-providers` and re-exports every moved symbol at its former `parish_inference::*` path, so downstream consumers need no import changes.                                                                                                               |
| `parish-providers`   | LLM **transport** half (split out of `parish-inference`): provider HTTP clients (`openai_client/`, `anthropic_client/`), shared client state + UTF-8 stream decoder (`client_base.rs`, `utf8_stream.rs`), unified dispatch (`any_client.rs` — `AnyClient`, `build_client`), offline `simulator.rs` (Markov) + scriptable `mock_client.rs` test backends, and outbound rate limiting (`rate_limit.rs`, `governor`). `reqwest` (json + stream) and `governor` are contained here. Backend-agnostic; must never depend on `parish-inference`.                                                                                        |
| `parish-input`       | Player input parsing & command detection, split across six modules: `commands.rs` (Command enum + validators), `intent_types.rs`, `parser.rs` (system commands + classification), `intent_local.rs` (keyword-matching pre-pass), `intent_llm.rs` (async LLM fallback), `mention.rs`.                                                                                                                                                                                                                                                                                                                                              |
| `parish-npc`         | NPC data model (`data.rs`, `types.rs`), mood (`mood.rs`), memory (`memory.rs`), scheduling (`ticks.rs`), autonomous speaker selection (`autonomous.rs`), overhear/witness memories (`overhear.rs`), reactions (`reactions.rs`), tier-4 rules engine (`tier4.rs`), anachronism detector (`anachronism.rs`), banshee death system (`banshee.rs`), transitions (`transitions.rs`), and the `NpcManager` (`manager.rs`).                                                                                                                                                                                                              |
| `parish-mod`         | Content-mod loader extracted from `parish-core/src/game_mod/`: manifest parsing (`manifest.rs`), discovery (`discovery.rs`), runtime data types (`types.rs`), asset-path validation (`assets.rs`), and the `parish-world` bridge (`world.rs`). Owns `GameMod`, `ModManifest`, `UiConfig`, `default_theme_palette()`, and provider-catalog loading. Backend-agnostic; re-exported by `parish-core` as `crate::game_mod`.                                                                                                                                                                                                           |
| `parish-editor`      | Parish Designer backend extracted from `parish-core/src/editor/`: deterministic atomic JSON writes (`format.rs`), editor DTOs (`types.rs`), granular file-by-file mod loading (`mod_io.rs`), cross-reference validation (`validate.rs`), validation-gated persistence (`persist.rs`), read-only save-file inspection (`save_inspect.rs`), and live world hot-reload (`live_reload.rs`). Backend-agnostic; depends only on leaf crates (`parish-mod`, `parish-world`, `parish-npc`, `parish-persistence`, `parish-types`) and is re-exported by `parish-core` as `crate::editor`.                                                  |
| `parish-palette`     | Day/night palette interpolation. Backend-agnostic presentation-layer infrastructure consumed by every UI surface; depends only on `parish-types` (Season/Weather) and `parish-config` (PaletteConfig). Owns the `From<RawPalette>` → `ThemePalette` hex conversion.                                                                                                                                                                                                                                                                                                                                                               |
| `parish-persistence` | SQLite save/load: `database.rs`, WAL journal (`journal.rs`, `journal_bridge.rs`), save picker (`picker.rs`), snapshot (`snapshot.rs`), file lock (`lock.rs`).                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `parish-world`       | World state: `graph.rs`, `movement.rs`, `description.rs`, `encounter.rs`, `geo.rs`, `transport.rs`, `weather.rs`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `parish-types`       | Shared primitive types: `error.rs` (`ParishError` via `thiserror`), `ids.rs`, `time.rs`, `events.rs`, `conversation.rs`, `dice.rs`, `gossip.rs`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `parish-geo-tool`    | OSM extraction CLI (binary `parish-geo-tool`).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `parish-npc-tool`    | Build-time NPC authoring tool (binary `parish-npc-tool`).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `parish-harness`     | Game quality-control harness (binary `parish-harness`). Drives a running backend over HTTP (never links the runtime); LLM plays + LLM judges N-turn playtests, scores gate+axes, records findings, persists to its own SQLite DB. Entry-point/tool crate — may use `axum`/`reqwest`; depends only on `parish-core`/`parish-inference`.                                                                                                                                                                                                                                                                                            |

## Repository layout

```text
Rundale (on Parish engine)/
├── parish/                 # Engine code (Rust workspace + frontends)
│   ├── crates/                 # 20 workspace members (see table above)
│   │
│   ├── apps/
│   │   └── ui/                 # Svelte 5 + TypeScript frontend (SvelteKit static adapter)
│   │       └── src/
│   │           ├── lib/                # types, ipc, map projection, label collision
│   │           ├── stores/             # game, theme, debug
│   │           └── components/         # StatusBar, ChatPanel, MapPanel, FullMapOverlay,
│   │                                   # Sidebar, InputField, SavePicker, DebugPanel
│   │
│   ├── testing/
│   │   └── fixtures/           # Plaintext script-mode fixtures (test_*.txt, play_*.txt)
│   │
│   ├── assets/                 # Binary assets (fonts, doc images)
│   │
│   ├── scripts/                # Maintenance scripts (doc-consistency checks, etc.)
│   │
│   ├── Cargo.toml              # Workspace manifest
│   ├── Cargo.lock
│   ├── justfile                # Task recipes
│   ├── parish.example.toml     # Example config
│   ├── about.toml              # About dialog data
│   └── about.hbs               # About dialog template
│
├── mods/
│   └── rundale/            # Rundale game content: 1820 rural Ireland
│       ├── mod.toml                # Manifest
│       ├── world.json              # Locations + connections
│       ├── npcs.json               # NPC definitions
│       ├── prompts/                # LLM prompt templates
│       ├── anachronisms.json       # Period enforcement dictionary
│       ├── festivals.json          # Calendar events
│       ├── encounters.json         # Encounter text
│       ├── loading.toml            # Spinner config
│       ├── ui.toml                 # Sidebar labels, accent colour
│       ├── transport.toml          # Transport rules
│       └── pronunciations.json     # Irish name phonetic guides
│
├── deploy/
│   └── Dockerfile          # Web-server build (build context: repo root)
│
└── docs/                   # See docs/index.md
    ├── agent/              # Agent docs (this directory)
    ├── adr/                # Architecture decision records
    ├── design/             # Subsystem & architecture docs
    ├── plans/              # Implementation phase plans
    ├── requirements/       # Roadmap
    ├── research/           # Historical 1820 Ireland research
    ├── development/        # Contributor guides
    ├── reviews/            # Code review notes
    ├── archive/            # DESIGN.md (original monolithic design)
    └── screenshots/        # GUI screenshots
```

## Module ownership

All **shared game logic** lives in the workspace's leaf crates (`parish-config`, `parish-editor`, `parish-inference`, `parish-input`, `parish-mod`, `parish-npc`, `parish-palette`, `parish-persistence`, `parish-providers`, `parish-types`, `parish-world`). `parish-core` composes them into stable namespaces used by every binary: `crate::config::…`, `crate::dice::…`, `crate::editor::…`, `crate::error::…`, `crate::game_mod::…`, `crate::inference::…`, `crate::input::…`, `crate::npc::…`, `crate::palette::…`, `crate::persistence::…`, `crate::world::…`.

`parish-engine` re-exports `parish_core` via `pub use parish_core::*` in `parish/crates/parish-engine/src/lib.rs` and only adds binary-specific modules: `main.rs`, `headless.rs`, `testing.rs`, `app.rs`, `config.rs` (CLI overrides on top of `parish_config`), `debug.rs`.

**Never create modules in `parish/crates/parish-engine/src/` that duplicate logic living in a leaf crate** — extend the leaf crate and re-export if needed.

## Mode parity

All modes (Tauri, CLI/headless, Axum web server, future modes) must have feature parity. Never add a feature to one mode that should apply to all. Implement shared logic in a leaf crate + re-export from `parish-core`, then wire it from every entry point (`parish/crates/parish-tauri/src/commands.rs`, `parish/crates/parish-server/src/routes.rs`, `parish/crates/parish-engine/src/headless.rs`, `parish/crates/parish-engine/src/testing.rs`).

`parish-client` is **not** an entry point — it's a downstream consumer of the HTTP API. Any new gameplay command exposed on `POST /api/command` automatically reaches `parish-client`, MCP, and the Svelte UI; no separate wiring required. Conversely, do not put gameplay logic in `parish-client` itself — it owns rendering and HTTP transport only.

## Idempotency

See [docs/agent/idempotency.md](idempotency.md) for the full spec.

The HTTP server implements `Idempotency-Key` replay (#619) for mutating routes via
`middleware::idempotency_middleware` in `parish/crates/parish-server/src/middleware.rs`.

**Supported routes** (POST):

| Route                     | Handler                      |
| ------------------------- | ---------------------------- |
| `POST /api/save-game`     | `routes::save_game`          |
| `POST /api/create-branch` | `routes::create_branch`      |
| `POST /api/new-save-file` | `routes::new_save_file`      |
| `POST /api/new-game`      | `routes::new_game`           |
| `POST /api/editor-save`   | `editor_routes::editor_save` |

**Cache:** process-wide LRU, capacity 1 000 entries, TTL 24 h. Stored on `GlobalState::idempotency_cache`.

**Feature flag:** `idempotency-key` — default-on; disable via `parish-flags.json`.

## Session capacity

The web server (`parish-server`) keeps one `SessionEntry` in memory per active visitor. Each entry holds a full copy of the game state: world graph, NPC manager, inference queue, and associated tick tasks. Memory usage is approximately:

```text
sessions * ~50 MB = total per-process memory footprint
```

### Admission-control ceiling (#620)

`GlobalState.max_concurrent_sessions` caps the number of live in-memory sessions per process. When the ceiling is reached, new session creation is refused with `503 Service Unavailable` and a `Retry-After: 30` header. Returning visitors (whose session is already in memory or can be restored from the DB) are never refused.

**Configuration** (resolution order, highest wins):

1. `PARISH_MAX_SESSIONS` environment variable (`usize`).
2. `[engine.session] max_concurrent_sessions` in `parish.toml`.
3. Compiled-in default: **50**.

**Feature flag**: `admission-control` — default-on (use `is_disabled` to kill-switch). Set via `parish-flags.json` in the data directory.

**Mode parity**: admission control is server-only by nature — the CLI and Tauri desktop modes have a single session per process and do not enforce a cap. `SessionRegistry::is_at_capacity` is only called from the server middleware.

### Stale-session eviction

Sessions inactive for more than 1 day are evicted from the in-memory `DashMap` (the cookie remains valid; the next visit restores from `saves/sessions.db`). Sessions inactive for more than 30 days are purged from disk (DB row + `saves/<id>/` directory) by a background task that runs hourly.

## Documentation Map

Start at [docs/index.md](../index.md) for the full hub. Key paths:

- **Architecture & design**: `docs/design/overview.md` → subsystem docs
- **Architecture decisions**: `docs/adr/README.md` → individual ADRs
- **Status tracking**: `docs/requirements/roadmap.md`
- **Implementation plans**: `docs/plans/`
- **Testing harness**: `docs/design/testing.md`
- **Dev journal**: `docs/archive/journal.md`
- **Known issues**: `docs/archive/known-issues.md`
- **Original design**: `docs/archive/DESIGN.md` (superseded)
