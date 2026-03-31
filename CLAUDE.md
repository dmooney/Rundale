# Parish — Claude Code Guide

## Build & Test

- Build: `cargo build`
- Release build: `cargo build --release`
- Run: `cargo run`
- Test all: `cargo test`
- Test one: `cargo test <test_name>`
- Format: `cargo fmt --check` (apply: `cargo fmt`)
- Lint: `cargo clippy -- -D warnings`
- Game harness: `cargo run -- --script tests/fixtures/test_walkthrough.txt`
- Frontend tests: `cd ui && npx vitest run` (or `just ui-test`)
- E2E GUI tests: `cd ui && npx playwright test` (or `just ui-e2e`)
- Screenshots: `cd ui && npx playwright test e2e/screenshots.spec.ts` (or `just screenshots`)

Use `/check` for quality gates, `/verify` for the full pre-push checklist, or `/game-test` for harness testing. Hooks handle formatting, compile checks, and quality gates automatically.

## Web Server Mode (for Browser Testing)

Run the Svelte frontend in a standard browser (no Tauri required):

```sh
# Build the frontend first
cd ui && npm run build && cd ..

# Start the web server on port 3001 (default)
cargo run -- --web
# Or specify a custom port
cargo run -- --web 8080
```

Then open `http://localhost:3001` in Chrome. The web server provides the same
game experience as the Tauri desktop app using HTTP + WebSocket instead of IPC.

### E2E Testing with Playwright

```sh
cd ui
npx playwright install chromium   # first time only
npx playwright test               # runs smoke tests in headless Chrome
npx playwright test --headed      # runs with visible browser
```

Playwright auto-starts the axum server via `cargo run -- --web 3099`.

## Engineering Standards

- All new code must have accompanying unit tests. No `#[allow]` without a justifying comment.
- Coverage must stay above **90%** (`cargo tarpaulin`).
- Hooks enforce: formatting, clippy, tests, doc updates, conventional commits, coverage reminders, and screenshot regeneration. See the **Hooks** section below.

## Architecture

See [docs/design/overview.md](docs/design/overview.md) for full architecture. See [docs/index.md](docs/index.md) for all documentation.

This is a **Cargo workspace** with four members:

```
Parish/
├── src/                 # Root crate: headless, testing, CLI entry point
│   ├── main.rs          #   Entry point, CLI args (clap), mode routing
│   ├── lib.rs           #   Module declarations + re-exports from parish-core
│   ├── headless.rs      #   Headless stdin/stdout REPL mode
│   ├── testing.rs       #   GameTestHarness for automated testing
│   ├── debug.rs         #   Debug commands and metrics (feature-gated)
│   ├── app.rs           #   Core application state (App, ScrollState)
│   └── bin/geo_tool/    #   OSM geographic data extraction tool
├── crates/parish-core/  # Pure game logic library (no UI dependencies)
│   └── src/
│       ├── error.rs     #   ParishError (thiserror)
│       ├── config.rs    #   Provider configuration (TOML + env + CLI)
│       ├── debug_snapshot.rs # DebugSnapshot struct + builder (debug data for GUI)
│       ├── game_mod.rs  #   GameMod loader (mod.toml manifest, data files, prompts)
│       ├── loading.rs   #   LoadingAnimation (configurable from mod or defaults)
│       ├── ipc/         #   Shared IPC types + handler functions (used by Tauri + web)
│       │   ├── types.rs #     WorldSnapshot, MapData, NpcInfo, ThemePalette, payloads
│       │   └── handlers.rs #  snapshot_from_world, build_map_data, build_theme, etc.
│       ├── input/       #   Player input parsing, command detection
│       ├── world/       #   World state, location graph, time, movement, encounters
│       │   ├── graph.rs #     WorldGraph, BFS pathfinding, fuzzy name search
│       │   ├── time.rs  #     GameClock, GameSpeed, TimeOfDay, Season, data-driven festivals
│       │   ├── palette.rs #   Smooth color interpolation (time/season/weather tinting)
│       │   ├── movement.rs #  Movement resolution and travel narration
│       │   ├── encounter.rs # En-route encounter system (hardcoded + mod-driven)
│       │   └── description.rs # Dynamic location description templates
│       ├── npc/         #   NPC data model, behavior, cognition tiers
│       │   └── anachronism.rs # Anachronism detection (hardcoded + mod-driven)
│       ├── inference/   #   LLM client (OpenAI-compatible), queue, Ollama bootstrap
│       └── persistence/ #   SQLite save/load, WAL journal, save picker
├── crates/parish-server/ # Axum web server for browser testing (no Tauri dependency)
│   └── src/
│       ├── lib.rs       #   run_server(), background ticks, client init
│       ├── state.rs     #   AppState, EventBus, GameConfig
│       ├── routes.rs    #   HTTP route handlers (REST API)
│       ├── ws.rs        #   WebSocket event relay
│       └── streaming.rs #   NPC token streaming via EventBus
├── mods/                # Game data packages (Factorio-style engine/mod separation)
│   └── kilteevan-1820/  # Default mod: 1820 rural Ireland
│       ├── mod.toml     #   Manifest (start_date, start_location, period_year)
│       ├── world.json   #   World graph (locations, connections)
│       ├── npcs.json    #   NPC definitions
│       ├── prompts/     #   LLM prompt templates with {placeholder} interpolation
│       ├── anachronisms.json # Period enforcement dictionary
│       ├── festivals.json    # Calendar events
│       ├── encounters.json   # Encounter text by time-of-day
│       ├── loading.toml      # Spinner frames, phrases, colours
│       └── ui.toml           # Sidebar labels, accent colour
├── src-tauri/           # Tauri 2 desktop backend (Rust)
│   └── src/
│       ├── lib.rs       #   AppState, IPC types, Tauri run() entry point
│       ├── main.rs      #   Tauri binary entry point
│       ├── commands.rs  #   Tauri IPC commands (get_world_snapshot, get_ui_config, submit_input, etc.)
│       └── events.rs    #   Event constants, streaming bridge (NPC token streaming)
└── ui/                  # Svelte 5 + TypeScript frontend (SvelteKit + static adapter)
    └── src/
        ├── lib/
        │   ├── types.ts #   TypeScript IPC types (snake_case, matching Rust serde)
        │   └── ipc.ts   #   Typed wrappers for all Tauri commands and events
        ├── stores/
        │   ├── game.ts  #   worldState, mapData, npcsHere, textLog, streamingActive, uiConfig
        │   ├── theme.ts #   palette store (applies CSS vars to :root)
        │   └── debug.ts #   debugVisible, debugSnapshot, debugTab, selectedNpcId
        └── components/
            ├── StatusBar.svelte  # Location | time | weather | season bar + debug toggle
            ├── ChatPanel.svelte  # Scrolling chat log with streaming cursor + Celtic knot spinner
            ├── MapPanel.svelte   # SVG equirectangular map with click-to-travel
            ├── Sidebar.svelte    # NPCs Here + Focail (Irish words) panels
            ├── InputField.svelte # Player input (disabled during streaming)
            └── DebugPanel.svelte # Tabbed debug panel (Overview, NPCs, World, Events, Inference)
```

## Code Style

- Doc comments (`///`) on all public structs and functions
- Use `thiserror` for library errors, `anyhow` in main/binary code
- Prefer `match` over `if let` for enum exhaustiveness
- Keep modules focused — one responsibility per file

## Key Dependencies

| Crate / Package | Purpose |
|-----------------|---------|
| tokio | Async runtime (features = "full") |
| tauri 2 | Desktop GUI framework (Rust backend + WebView frontend) |
| @tauri-apps/api v2 | TypeScript IPC bindings |
| svelte 5 + sveltekit | Frontend framework (static adapter for Tauri) |
| reqwest | HTTP client for Ollama/LLM API |
| serde + serde_json | JSON serialization for LLM structured output |
| rusqlite | SQLite persistence (features = "bundled") |
| anyhow / thiserror | Error handling |
| tracing | Structured logging |
| chrono | Time representation |
| vitest + @testing-library/svelte | Frontend component tests |

## Gotchas

- **Tokio + blocking**: Never use `std::thread::sleep` in async code; use `tokio::time::sleep`
- **Rusqlite is sync**: Wrap DB calls in `tokio::task::spawn_blocking`
- **Ollama**: Must be running on `localhost:11434` for inference calls
- **Reqwest timeouts**: Set explicit timeouts on all HTTP requests
- **Serde defaults**: Use `#[serde(default)]` for optional fields in LLM response structs

## Git Workflow

- Conventional commits: `feat:`, `fix:`, `refactor:`, `docs:`, `test:` (enforced by `commit-msg-check.sh` hook)
- One logical change per commit
- Run full test suite before pushing

## GUI Screenshots

Screenshots live in `docs/screenshots/` and are referenced from `README.md`.

**Always regenerate screenshots when you change anything in `ui/` or `src-tauri/`.** Run:

```sh
cd ui && npx playwright test e2e/screenshots.spec.ts
# or: just screenshots
```

This captures the Svelte frontend at 4 times of day (morning, midday, dusk, night) using Playwright with headless Chromium and mocked Tauri IPC. No X11, GDK, or `xvfb` required.

To update visual regression baselines after intentional UI changes:

```sh
cd ui && npx playwright test --update-snapshots
# or: just ui-e2e-update
```

Commit the updated screenshots alongside your UI changes.

## Tauri Development

- Dev: `cargo tauri dev` (starts Vite + Tauri together)
- Build: `cargo tauri build`
- IPC types use `snake_case` — TypeScript types in `ui/src/lib/types.ts` must match Rust serde output exactly.
- System requirements (Linux): `libgtk-3-dev`, `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`, `patchelf`.

## Documentation Map

Start at [docs/index.md](docs/index.md) for the full hub. Key paths:

- **Architecture & design**: `docs/design/overview.md` → subsystem docs
- **Architecture decisions**: `docs/adr/README.md` → individual ADRs
- **Status tracking**: `docs/requirements/roadmap.md` (per-item checkboxes)
- **Implementation plans**: `docs/plans/` (one per phase)
- **Testing harness**: `docs/design/testing.md` (GameTestHarness, script mode)
- **Dev journal**: `docs/journal.md` (cross-session notes)
- **Known issues**: `docs/known-issues.md`
- **Archival**: `DESIGN.md` (original monolithic design, superseded by `docs/design/`)

## Claude Code Skills

Custom slash commands defined in `.claude/skills/`:

| Skill | Description |
|-------|-------------|
| `/check` | Run fmt + clippy + tests (quality gate) |
| `/game-test [script]` | Run GameTestHarness to verify game behavior |
| `/verify` | Full pre-push checklist (quality gate + harness) |
| `/screenshot` | Regenerate GUI screenshots via Playwright (headless Chromium) |
| `/fix-issue <N>` | End-to-end GitHub issue workflow |
| `/chrome-test` | Live Chrome browser testing session via Claude-in-Chrome MCP |

## Claude Code Hooks

Automated hooks configured in `.claude/settings.json` that run at lifecycle events:

| Hook | Event | Trigger | What It Does |
|------|-------|---------|--------------|
| `auto-fmt.sh` | PostToolUse | After any Edit/Write to a `.rs` file | Runs `cargo fmt --quiet` to auto-format |
| `compile-check.sh` | PostToolUse | After any Edit/Write to a `.rs` file | Runs `cargo check` for immediate compile error feedback |
| `dep-audit-reminder.sh` | PostToolUse | After any Edit/Write to `Cargo.toml` | Reminds to run `cargo audit` and `cargo outdated` |
| `protect-files.sh` | PreToolUse | Before any Edit/Write | Blocks direct edits to `Cargo.lock` (exit 2) |
| `quality-gates.sh` | Stop | When Claude finishes responding | Runs fmt + clippy + test if `.rs` files changed |
| `harness-reminder.sh` | Stop | When Claude finishes responding | Reminds to run game harness if parish-core/world logic changed |
| `doc-staleness.sh` | Stop | When Claude finishes responding | Warns if `.rs` files changed but no docs were updated |
| `screenshot-reminder.sh` | Stop | When Claude finishes responding | Reminds to regenerate screenshots if `ui/` or `src-tauri/` changed |
| `coverage-reminder.sh` | Stop | When Claude finishes responding | Reminds to check coverage when new `.rs` files are added |
| `design-doc-reminder.sh` | Stop | When Claude finishes responding | Reminds to update `docs/design/` when new public structs/functions/modules are added |
| `compact-context.sh` | SessionStart | After context compaction | Re-injects key project context |
| `commit-msg-check.sh` | UserPromptSubmit | When user submits a prompt mentioning "commit" | Validates conventional commit message format |
| `notify.sh` | Notification | When Claude needs attention | Sends desktop notification via `notify-send` |
| `worktree-compile.sh` | WorktreeCreate | When a git worktree is created | Runs `cargo check --all` to verify workspace compiles |
| `tauri-server-check.sh` | SubagentStart | When a subagent is spawned | Checks if Vite/Tauri dev server is running |

Hook scripts live in `.claude/hooks/` and require `jq` for JSON parsing. All scripts are executable (`chmod +x`).
