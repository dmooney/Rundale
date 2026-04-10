# Architecture & Layout

See [docs/design/overview.md](../design/overview.md) for the full architecture and [docs/index.md](../index.md) for all documentation.

Parish is a **Cargo workspace**. All Rust crates live under `crates/`, all frontends under `apps/`, all test fixtures under `testing/`, all deploy artifacts under `deploy/`.

```
Parish/
├── crates/
│   ├── parish-core/         # Pure game logic library (no UI deps)
│   │   └── src/
│   │       ├── error.rs            # ParishError (thiserror)
│   │       ├── config/             # Engine + provider configuration (TOML + env + CLI)
│   │       ├── debug_snapshot.rs   # DebugSnapshot for the GUI
│   │       ├── game_mod.rs         # GameMod loader (mod.toml manifest, data files, prompts)
│   │       ├── loading.rs          # LoadingAnimation
│   │       ├── ipc/                # Shared IPC types + handler functions (used by all backends)
│   │       │   ├── types.rs
│   │       │   ├── handlers.rs
│   │       │   ├── commands.rs
│   │       │   ├── config.rs
│   │       │   └── streaming.rs
│   │       ├── input/              # Player input parsing & command detection
│   │       ├── world/              # World state, graph, time, movement, encounters, weather
│   │       ├── npc/                # NPC data model, behavior, cognition tiers, memory
│   │       ├── inference/          # LLM client, queue, Ollama bootstrap
│   │       └── persistence/        # SQLite save/load, WAL journal, save picker
│   │
│   ├── parish-cli/          # Headless / web / CLI entry point (binary name: `parish`)
│   │   ├── src/
│   │   │   ├── main.rs             # CLI args (clap), mode routing
│   │   │   ├── lib.rs              # Re-exports from parish-core
│   │   │   ├── headless.rs         # Headless stdin/stdout REPL
│   │   │   ├── testing.rs          # GameTestHarness (script mode)
│   │   │   ├── debug.rs            # Debug commands & metrics
│   │   │   ├── config.rs           # CLI override layer over engine config
│   │   │   └── app.rs              # Core App / ScrollState
│   │   └── tests/                  # Integration tests (load fixtures from ../../testing/fixtures)
│   │
│   ├── parish-server/       # Axum web server backend (no Tauri dep)
│   │   └── src/
│   │       ├── lib.rs              # run_server(), background ticks, client init
│   │       ├── state.rs            # AppState, EventBus, GameConfig
│   │       ├── routes.rs           # HTTP handlers (REST API)
│   │       ├── ws.rs               # WebSocket event relay
│   │       └── streaming.rs        # NPC token streaming via EventBus
│   │
│   ├── parish-tauri/        # Tauri 2 desktop backend
│   │   ├── tauri.conf.json         # frontendDist → ../../apps/ui/dist
│   │   └── src/
│   │       ├── lib.rs              # AppState, IPC types, run() entry
│   │       ├── main.rs             # Tauri binary
│   │       ├── commands.rs         # Tauri IPC commands
│   │       └── events.rs           # Streaming bridge
│   │
│   └── geo-tool/            # OSM extraction CLI (binary name: `geo-tool`)
│
├── apps/
│   └── ui/                  # Svelte 5 + TypeScript frontend (SvelteKit static adapter)
│       └── src/
│           ├── lib/                # types, ipc, map projection, label collision
│           ├── stores/             # game, theme, debug
│           └── components/         # StatusBar, ChatPanel, MapPanel, FullMapOverlay,
│                                   # Sidebar, InputField, SavePicker, DebugPanel
│
├── testing/
│   └── fixtures/            # Plaintext script-mode fixtures (test_*.txt, play_*.txt)
│
├── mods/
│   └── rundale/      # Default mod: 1820 rural Ireland
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
├── assets/                  # Binary assets (fonts, doc images)
│
├── deploy/
│   ├── Dockerfile           # Web-server build (build context: repo root)
│   └── railway.toml         # Railway deployment config
│
└── docs/                    # See docs/index.md
    ├── agent/               # Agent docs (this directory)
    ├── adr/                 # Architecture decision records
    ├── design/              # Subsystem & architecture docs
    ├── plans/               # Implementation phase plans
    ├── requirements/        # Roadmap
    ├── research/            # Historical 1820 Ireland research
    ├── development/         # Contributor guides
    ├── reviews/             # Code review notes
    ├── archive/             # DESIGN.md (original monolithic design)
    └── screenshots/         # GUI screenshots
```

## Module ownership

All shared game logic — `world`, `npc`, `inference`, `input`, `ipc`, `persistence`, `error`, `loading`, `game_mod`, `config` — lives **exclusively** in `crates/parish-core/`. The `parish-cli` crate re-exports these via `pub use parish_core::*` in `src/lib.rs`. **Never create duplicate modules in `crates/parish-cli/src/`** — modify `parish-core` instead.

`parish-cli/src/` only contains binary-specific code: `main.rs`, `headless.rs`, `testing.rs`, `app.rs`, `config.rs` (CLI overrides), `debug.rs`.

## Mode parity

All modes (Tauri, CLI/headless, web server, future modes) must have feature parity. Never add a feature to one mode that should apply to all. Implement shared logic in `parish-core/` and wire it from every entry point.

## Documentation Map

Start at [docs/index.md](../index.md) for the full hub. Key paths:

- **Architecture & design**: `docs/design/overview.md` → subsystem docs
- **Architecture decisions**: `docs/adr/README.md` → individual ADRs
- **Status tracking**: `docs/requirements/roadmap.md`
- **Implementation plans**: `docs/plans/`
- **Testing harness**: `docs/design/testing.md`
- **Dev journal**: `docs/journal.md`
- **Known issues**: `docs/known-issues.md`
- **Original design**: `docs/archive/DESIGN.md` (superseded)
