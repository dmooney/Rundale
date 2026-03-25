# Parish — Claude Code Guide

## Build & Test

- Build: `cargo build`
- Release build: `cargo build --release`
- Run: `cargo run`
- Test all: `cargo test`
- Test one: `cargo test <test_name>`
- Format: `cargo fmt --check` (apply: `cargo fmt`)
- Lint: `cargo clippy -- -D warnings`

Run `cargo fmt`, `cargo clippy`, and `cargo test` before committing.

**Skills shortcut**: Run `/check` to execute all three quality gates, or `/verify` for the full pre-push checklist including the game harness.

## Verification Before Pushing

**Always manually verify changes work before pushing.** Running tests alone is not enough — use the `GameTestHarness` to actually exercise your changes:

- Run `cargo run -- --script tests/fixtures/test_walkthrough.txt` and inspect the JSON output
- Write a quick ad-hoc script file to test the specific feature you changed
- If you added or changed game mechanics, write a targeted test script and run it through `--script` mode
- Only push after you've both run the test suite **and** visually confirmed the harness output looks correct

## Engineering Standards

Every commit **must** satisfy all of the following:

1. **Documentation**: **Every commit must leave docs current.** Update `README.md`, `CLAUDE.md`, `docs/design/`, `docs/adr/`, and doc comments (`///`) to reflect all changes. New public APIs, changed behavior, renamed or removed items, and architectural decisions must be documented before pushing. If you change code, you change the docs — no exceptions.
2. **Tests required**: All new code must have accompanying unit tests. No new function, struct, or module lands without test coverage.
3. **Coverage threshold**: Maintain test coverage above **90%**. Use `cargo tarpaulin` (or equivalent) to verify. PRs that drop coverage below 90% must not be merged.
4. **All standards must pass**: `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test` must all succeed. No exceptions, no `#[allow]` without a justifying comment.

## Architecture

See [docs/design/overview.md](docs/design/overview.md) for full architecture. See [docs/index.md](docs/index.md) for all documentation.

This is a **Cargo workspace** with three members:

```
Parish/
├── src/                 # Root crate: TUI, headless, testing, CLI entry point
│   ├── main.rs          #   Entry point, CLI args (clap), mode routing
│   ├── lib.rs           #   Module declarations + re-exports from parish-core
│   ├── headless.rs      #   Headless stdin/stdout REPL mode
│   ├── testing.rs       #   GameTestHarness for automated testing
│   ├── debug.rs         #   Debug commands and metrics (feature-gated)
│   ├── tui/             #   Ratatui terminal UI + debug panel
│   └── bin/geo_tool/    #   OSM geographic data extraction tool
├── crates/parish-core/  # Pure game logic library (no UI dependencies)
│   └── src/
│       ├── error.rs     #   ParishError (thiserror)
│       ├── config.rs    #   Provider configuration (TOML + env + CLI)
│       ├── loading.rs   #   LoadingAnimation (RGB-based, no ratatui)
│       ├── input/       #   Player input parsing, command detection
│       ├── world/       #   World state, location graph, time, movement, encounters
│       │   ├── graph.rs #     WorldGraph, BFS pathfinding, fuzzy name search
│       │   ├── time.rs  #     GameClock, GameSpeed, TimeOfDay, Season
│       │   ├── palette.rs #   Smooth color interpolation (time/season/weather tinting)
│       │   ├── movement.rs #  Movement resolution and travel narration
│       │   ├── encounter.rs # En-route encounter system
│       │   ├── events.rs  #     Cross-tier event bus (tokio broadcast)
│       │   ├── weather.rs #     Markov chain weather engine
│       │   └── description.rs # Dynamic location description templates
│       ├── npc/         #   NPC data model, behavior, cognition tiers
│       │   ├── ticks.rs   #   Tier 1-4 tick dispatch, inflate/deflate, seasonal effects
│       │   ├── memory.rs  #   Short-term (ring buffer) + long-term (keyword recall, 100 entries)
│       │   ├── gossip.rs  #   Gossip propagation with distortion
│       │   ├── tier3.rs   #   Tier 3 batch inference (daily)
│       │   ├── tier4.rs   #   Tier 4 rules engine (seasonal)
│       ├── inference/   #   LLM client (OpenAI-compatible), queue, Ollama bootstrap
│       └── persistence/ #   SQLite save/load, WAL journal
├── src-tauri/           # Tauri 2 desktop backend (Rust)
│   └── src/
│       ├── lib.rs       #   AppState, IPC types, Tauri run() entry point
│       ├── main.rs      #   Tauri binary entry point
│       ├── commands.rs  #   Tauri IPC commands (get_world_snapshot, submit_input, etc.)
│       └── events.rs    #   Event constants, streaming bridge (NPC token streaming)
└── ui/                  # Svelte 5 + TypeScript frontend (SvelteKit + static adapter)
    └── src/
        ├── lib/
        │   ├── types.ts #   TypeScript IPC types (snake_case, matching Rust serde)
        │   └── ipc.ts   #   Typed wrappers for all Tauri commands and events
        ├── stores/
        │   ├── game.ts  #   worldState, mapData, npcsHere, textLog, streamingActive
        │   └── theme.ts #   palette store (applies CSS vars to :root)
        └── components/
            ├── StatusBar.svelte  # Location | time | weather | season bar
            ├── ChatPanel.svelte  # Scrolling chat log with streaming cursor
            ├── MapPanel.svelte   # SVG equirectangular map with click-to-travel
            ├── Sidebar.svelte    # NPCs Here + Focail (Irish words) panels
            └── InputField.svelte # Player input (disabled during streaming)
```

## Code Style

- Follow `cargo fmt` output exactly
- All `cargo clippy` warnings are errors (`-D warnings`)
- Doc comments (`///`) on all public structs and functions
- Use `thiserror` for library errors, `anyhow` in main/binary code
- Prefer `match` over `if let` for enum exhaustiveness
- Keep modules focused — one responsibility per file

## Key Dependencies

| Crate / Package | Purpose |
|-----------------|---------|
| tokio | Async runtime (features = "full") |
| ratatui + crossterm | Terminal UI with 24-bit true color |
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
- **Ratatui panic safety**: Always restore terminal state on panic (install panic hook)
- **Ollama**: Must be running on `localhost:11434` for inference calls
- **Reqwest timeouts**: Set explicit timeouts on all HTTP requests
- **Serde defaults**: Use `#[serde(default)]` for optional fields in LLM response structs

## Git Workflow

- Conventional commits: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`
- One logical change per commit
- Run full test suite before pushing

## GUI Screenshots

Screenshots live in `docs/screenshots/` and are referenced from `README.md`.

**Always regenerate screenshots when you change anything in `ui/` or `src-tauri/`.** Run:

```sh
cargo tauri dev -- -- --screenshot docs/screenshots
```

This captures the Tauri WebView at 4 times of day (morning, midday, dusk, night) via `WebviewWindow::capture_image()` and saves PNGs.

Commit the updated screenshots alongside your UI changes.

## Tauri Development

**To run the full Tauri desktop app:**
```sh
cargo tauri dev
```
This starts the Vite dev server (`ui/`) and the Tauri backend (`src-tauri/`) together.

**To build a production bundle:**
```sh
cargo tauri build
```

**Frontend tests** (Svelte components):
```sh
cd ui && npm test
```

**IPC types**: All Rust → TypeScript types use `snake_case` (Rust serde defaults). TypeScript types in `ui/src/lib/types.ts` must match exactly.

**System requirements** (Linux): `libgtk-3-dev`, `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`, `patchelf`.

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
| `/screenshot` | Regenerate GUI screenshots via xvfb |
| `/fix-issue <N>` | End-to-end GitHub issue workflow |
