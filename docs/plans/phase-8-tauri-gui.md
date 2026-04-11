# Plan: Phase 8 — Tauri GUI Rewrite

> Parent: [Roadmap](../requirements/roadmap.md) | [Docs Index](../index.md)
>
> **Status: Planned**

## Goal

Replace the egui/eframe windowed GUI (`src/gui/`) with a **Tauri 2 + Svelte** desktop application.
The Rust game engine becomes the Tauri backend; the UI is a Svelte 5 + TypeScript SPA bundled by
Vite. All game state crosses the boundary via typed Tauri commands and events. The TUI and headless
modes are preserved unchanged. This phase lays the foundation for Phase 7's mobile and web targets.

## Prerequisites

- Phase 3 complete (NPC system, world graph) ✓
- ADR-015 accepted ✓
- `xvfb-run`, Node.js ≥ 20, `npm` or `pnpm`, and the Tauri CLI (`cargo install tauri-cli`) available in the dev environment

## Workspace Structure (after migration)

```
parish/
├── Cargo.toml                    ← workspace manifest
├── CLAUDE.md                     ← updated build/test instructions
├── crates/
│   └── parish-core/              ← extracted game-logic library
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── error.rs
│           ├── config.rs
│           ├── headless.rs
│           ├── testing.rs
│           ├── debug.rs
│           ├── input/
│           ├── world/
│           ├── npc/
│           ├── inference/
│           └── persistence/
├── src-tauri/                    ← Tauri backend crate
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── capabilities/
│   │   └── default.json
│   └── src/
│       ├── main.rs               ← minimal Tauri entry point
│       ├── lib.rs                ← app setup, state, command registration
│       ├── commands.rs           ← #[tauri::command] handlers
│       └── events.rs             ← event emission helpers + streaming bridge
├── src/                          ← CLI binary (TUI + headless)
│   └── main.rs
├── ui/                           ← Svelte frontend
│   ├── package.json
│   ├── vite.config.ts
│   ├── tsconfig.json
│   └── src/
│       ├── App.svelte
│       ├── components/
│       │   ├── ChatPanel.svelte
│       │   ├── MapPanel.svelte
│       │   ├── Sidebar.svelte
│       │   ├── StatusBar.svelte
│       │   └── InputField.svelte
│       ├── stores/
│       │   ├── game.ts           ← writable stores for world state
│       │   └── theme.ts          ← CSS variable store (time-of-day palette)
│       └── lib/
│           ├── ipc.ts            ← typed invoke() + listen() wrappers
│           └── types.ts          ← TypeScript mirrors of Rust structs
├── data/                         ← unchanged
└── tests/fixtures/               ← unchanged
```

## New Dependencies

### Rust (workspace Cargo.toml)

| Crate | Version | Purpose |
|-------|---------|---------|
| `tauri` | 2 | Tauri app framework, command/event system |
| `tauri-build` | 2 | Build-script helper for Tauri metadata |
| `serde` | 1 | Serialise command return types and event payloads |

Remove: `eframe = "0.31"` and `image` (PNG encoding only needed for egui screenshots).

### JavaScript (`ui/package.json`)

| Package | Purpose |
|---------|---------|
| `@tauri-apps/api` | Typed `invoke()` and `listen()` bindings |
| `@tauri-apps/plugin-*` | Shell, dialog, fs plugins as needed |
| `svelte` | UI framework |
| `@sveltejs/vite-plugin-svelte` | Vite integration |
| `vite` | Dev server + bundler |
| `typescript` | Type safety |

## Tasks

### Part A: Workspace & Core Extraction

1. **Convert `Cargo.toml` to a workspace manifest**
   - Replace `[package]` with `[workspace]` containing `members = ["crates/parish-core", "src-tauri", "."]`
   - Keep the root `[[bin]]` entries for `parish` (CLI) and `geo-tool`
   - Move all current `[dependencies]` (minus `eframe` and `image`) to `crates/parish-core/Cargo.toml`
   - Root `Cargo.toml` depends on `parish-core = { path = "crates/parish-core" }`

2. **Create `crates/parish-core/` library crate**
   - Move `src/{error,config,headless,testing,debug}.rs` and `src/{input,world,npc,inference,persistence}/` into `crates/parish-core/src/`
   - Update `crates/parish-core/src/lib.rs` to re-export all public modules
   - All internal `use crate::` paths remain valid; only the crate name changes for external consumers
   - Run `cargo test -p parish-core` — all existing tests must pass before proceeding

3. **Delete `src/gui/`**
   - Remove `src/gui/mod.rs`, `theme.rs`, `chat_panel.rs`, `map_panel.rs`, `sidebar.rs`,
     `status_bar.rs`, `input_field.rs`, `screenshot.rs`
   - Remove `pub mod gui;` from `src/lib.rs` (or `crates/parish-core/src/lib.rs`)
   - Remove the `gui::run_gui(...)` call from `src/main.rs`; the CLI binary now only launches TUI or headless mode
   - Confirm `cargo build` still succeeds for the CLI binary

### Part B: Tauri Backend

4. **Initialise the Tauri crate at `src-tauri/`**
   - Run `cargo tauri init` from the repo root, pointing devUrl at `http://localhost:5173` and distDir at `../ui/dist`
   - Edit `src-tauri/Cargo.toml`: add `parish-core = { path = "../crates/parish-core" }` as a dependency
   - Set `productName = "Rundale"`, `version` from workspace, `identifier = "ie.parish.app"` in `tauri.conf.json`

5. **Define the shared IPC type surface in `src-tauri/src/lib.rs`**
   Serde-serialisable structs mirrored in `ui/src/lib/types.ts`:
   ```rust
   #[derive(serde::Serialize, Clone)]
   pub struct WorldSnapshot {
       pub location_name: String,
       pub location_description: String,
       pub time_label: String,       // "Morning", "Dusk", etc.
       pub hour: u8,
       pub weather: String,
       pub season: String,
       pub festival: Option<String>,
       pub paused: bool,
   }

   #[derive(serde::Serialize, Clone)]
   pub struct MapData {
       pub locations: Vec<MapLocation>,
       pub edges: Vec<(String, String)>,
       pub player_location: String,
   }

   #[derive(serde::Serialize, Clone)]
   pub struct MapLocation {
       pub id: String,
       pub name: String,
       pub lat: f64,
       pub lon: f64,
       pub adjacent: bool,     // reachable from current position
   }

   #[derive(serde::Serialize, Clone)]
   pub struct NpcInfo {
       pub name: String,
       pub occupation: String,
       pub mood: String,
   }

   #[derive(serde::Serialize, Clone)]
   pub struct ThemePalette {
       pub bg: String,          // "#rrggbb"
       pub fg: String,
       pub accent: String,
       pub panel_bg: String,
       pub input_bg: String,
       pub border: String,
       pub muted: String,
   }
   ```

   Hold mutable game state in a `tauri::State`-managed struct:
   ```rust
   pub struct AppState {
       pub world: tokio::sync::Mutex<WorldState>,
       pub npc_manager: tokio::sync::Mutex<NpcManager>,
       pub inference_queue: tokio::sync::Mutex<Option<InferenceQueue>>,
       pub client: Option<OpenAiClient>,
       pub cloud_client: Option<OpenAiClient>,
       pub streaming_active: Arc<tokio::sync::Mutex<bool>>,
   }
   ```

6. **Implement Tauri commands in `src-tauri/src/commands.rs`**
   ```rust
   #[tauri::command]
   pub async fn submit_input(text: String, state: tauri::State<'_, AppState>,
       app: tauri::AppHandle) -> Result<(), String>

   #[tauri::command]
   pub async fn get_world_snapshot(state: tauri::State<'_, AppState>)
       -> Result<WorldSnapshot, String>

   #[tauri::command]
   pub async fn get_map(state: tauri::State<'_, AppState>)
       -> Result<MapData, String>

   #[tauri::command]
   pub async fn get_npcs_here(state: tauri::State<'_, AppState>)
       -> Result<Vec<NpcInfo>, String>

   #[tauri::command]
   pub async fn get_theme(state: tauri::State<'_, AppState>)
       -> Result<ThemePalette, String>
   ```
   `submit_input` processes input through the `parish-core` pipeline (classify → movement or NPC
   conversation → inference) and emits events as the response streams.

7. **Implement the streaming event bridge in `src-tauri/src/events.rs`**
   - Define event payload types with `#[derive(serde::Serialize, Clone)]`:
     - `StreamTokenPayload { token: String }` → event name `"stream-token"`
     - `StreamEndPayload { hints: Vec<IrishWordHint> }` → event name `"stream-end"`
     - `TextLogPayload { source: String, content: String }` → event name `"text-log"`
     - `WorldUpdatePayload` (same fields as `WorldSnapshot`) → event name `"world-update"`
     - `LoadingPayload { active: bool }` → event name `"loading"`
   - Spawn a `tokio::task` inside `submit_input` that reads from `InferenceQueue`'s response
     channel and calls `app.emit("stream-token", payload)` for each token
   - Batch tokens: accumulate for 16 ms before emitting to reduce IPC round-trips
   - After stream end: parse Irish word hints from the buffered response, then emit `"stream-end"`
   - After each world state change (movement, time tick): emit `"world-update"`

8. **Wire up Tauri app setup in `src-tauri/src/lib.rs`**
   - Initialise `AppState` (load `data/parish.json`, `data/npcs.json`, set up inference clients)
   - Register commands: `tauri::generate_handler![submit_input, get_world_snapshot, get_map, get_npcs_here, get_theme]`
   - Start idle-tick background task (`tokio::spawn`) that fires every 20 s and emits `"world-update"`
   - Start theme-tick task that emits `"theme-update"` every 500 ms with the current palette

9. **Set Tauri capability permissions in `src-tauri/capabilities/default.json`**
   - Allow: `core:window:allow-start-dragging`, `core:app:allow-app-show`
   - No filesystem or shell permissions needed for the game itself

### Part C: Svelte Frontend

10. **Initialise the Svelte project in `ui/`**
    - `npm create svelte@latest ui` — choose Svelte 5, TypeScript, Vite, no SSR
    - Install `@tauri-apps/api`
    - Add `"@tauri-apps/api": "^2"` to `package.json`

11. **Write typed IPC wrappers in `ui/src/lib/ipc.ts`**
    ```typescript
    import { invoke } from "@tauri-apps/api/core";
    import { listen } from "@tauri-apps/api/event";
    import type { WorldSnapshot, MapData, NpcInfo, ThemePalette,
                  StreamTokenPayload, StreamEndPayload, TextLogPayload,
                  WorldUpdatePayload, LoadingPayload } from "./types";

    export const submitInput = (text: string) =>
        invoke<void>("submit_input", { text });
    export const getWorldSnapshot = () =>
        invoke<WorldSnapshot>("get_world_snapshot");
    export const getMap = () => invoke<MapData>("get_map");
    export const getNpcsHere = () => invoke<NpcInfo[]>("get_npcs_here");
    export const getTheme = () => invoke<ThemePalette>("get_theme");

    export const onStreamToken = (cb: (p: StreamTokenPayload) => void) =>
        listen<StreamTokenPayload>("stream-token", e => cb(e.payload));
    export const onStreamEnd = (cb: (p: StreamEndPayload) => void) =>
        listen<StreamEndPayload>("stream-end", e => cb(e.payload));
    export const onTextLog = (cb: (p: TextLogPayload) => void) =>
        listen<TextLogPayload>("text-log", e => cb(e.payload));
    export const onWorldUpdate = (cb: (p: WorldUpdatePayload) => void) =>
        listen<WorldUpdatePayload>("world-update", e => cb(e.payload));
    export const onLoading = (cb: (p: LoadingPayload) => void) =>
        listen<LoadingPayload>("loading", e => cb(e.payload));
    ```

12. **Implement Svelte stores in `ui/src/stores/`**

    `game.ts`:
    ```typescript
    import { writable, derived } from "svelte/store";
    import type { WorldSnapshot, MapData, NpcInfo, TextLogEntry } from "../lib/types";

    export const worldState = writable<WorldSnapshot | null>(null);
    export const mapData = writable<MapData | null>(null);
    export const npcsHere = writable<NpcInfo[]>([]);
    export const textLog = writable<TextLogEntry[]>([]);
    export const streamingActive = writable(false);
    export const irishHints = writable<IrishWordHint[]>([]);
    ```

    `theme.ts`:
    ```typescript
    import { writable } from "svelte/store";
    import type { ThemePalette } from "../lib/types";

    export const palette = writable<ThemePalette | null>(null);

    // Applies palette as CSS custom properties on :root
    palette.subscribe(p => {
        if (!p) return;
        const root = document.documentElement;
        root.style.setProperty("--color-bg", p.bg);
        root.style.setProperty("--color-fg", p.fg);
        root.style.setProperty("--color-accent", p.accent);
        root.style.setProperty("--color-panel-bg", p.panel_bg);
        root.style.setProperty("--color-input-bg", p.input_bg);
        root.style.setProperty("--color-border", p.border);
        root.style.setProperty("--color-muted", p.muted);
    });
    ```

13. **Implement `App.svelte` — root layout and event wiring**
    - On mount: call `getWorldSnapshot()`, `getMap()`, `getNpcsHere()`, `getTheme()` to populate stores
    - Subscribe to all five event streams (`onStreamToken`, `onStreamEnd`, `onTextLog`, `onWorldUpdate`, `onLoading`)
    - `onStreamToken`: append token to the last entry in `textLog` (streaming in-place)
    - `onStreamEnd`: set `streamingActive = false`, push hints to `irishHints`
    - `onWorldUpdate`: update `worldState` store; re-fetch `mapData` and `npcsHere` if location changed
    - Layout: CSS grid — status bar top, chat panel left/centre, map panel right, sidebar far-right, input field bottom

14. **Implement `StatusBar.svelte`**
    - Reactive to `$worldState`
    - Displays: `{location} | {time_label} {hour}:00 | {weather} | {season}` with optional festival badge
    - Uses `var(--color-accent)` for the festival/pause highlight
    - Matches the information density of the current `status_bar.rs`

15. **Implement `ChatPanel.svelte`**
    - Renders `$textLog` as a scrollable list; auto-scrolls to bottom on new entries
    - Each entry: speaker label in `var(--color-accent)`, body text in `var(--color-fg)`
    - Loading state (`$streamingActive`): shows animated Celtic triquetra (Trinity knot) SVG spinner — three interlocking lobes draw and erase sequentially using `stroke-dasharray`/`stroke-dashoffset` CSS animation with `pathLength="120"` normalization, staggered delays (0s/0.8s/1.6s), opacity pulsing (0.3→1→0.3), and a slow 6s rotation overlay. Uses `var(--color-accent)` (gold) for the stroke, adapting to time-of-day palette changes. Pure inline SVG + scoped CSS, no JS animation libraries or font glyph dependencies
    - Empty state: "The story begins…" in muted italic
    - Streaming entry: last log entry renders with a blinking cursor while `streamingActive` is true

16. **Implement `MapPanel.svelte`**
    - Renders `$mapData` as an SVG element
    - Project `lat`/`lon` to SVG viewport coordinates using a simple equirectangular projection
      bounded to the parish's geographic extent
    - Location nodes: `<circle>` elements; player location highlighted with `var(--color-accent)`;
      adjacent locations with dashed border; others muted
    - Edges: `<line>` elements in `var(--color-border)`
    - NPC presence: small dot above each location node where NPCs are present
    - Click on adjacent location → call `submitInput("go to {locationName}")`
    - Tooltip on hover: location name + NPC count

17. **Implement `Sidebar.svelte`**
    - Two collapsible sections matching `sidebar.rs`:
      - **Focail** (Words): list of `$irishHints`, each showing word / phonetic / meaning
      - **NPCs Here**: list from `$npcsHere` showing name / occupation / mood
    - Styled with `var(--color-panel-bg)` and `var(--color-border)`

18. **Implement `InputField.svelte`**
    - Single `<input type="text">` with placeholder "Type a command or speak…"
    - On Enter (or submit button): call `submitInput(text)`, clear field, set `streamingActive = true`
    - Disabled while `$streamingActive` is true to prevent double-submission
    - Auto-focus on mount and after each submission

19. **Implement the time-of-day theme in CSS**
    - All component styles use `var(--color-*)` custom properties exclusively — no hard-coded colours
    - Global `ui/src/app.css`: base reset, font stack (system-ui with Irish-friendly fallbacks),
      scrollbar styling, CSS transitions on `--color-*` (500 ms ease) for smooth time-of-day shifts
    - Typography: use `@font-face` to load a suitable Irish/Celtic display font for headings
      (e.g. `IM Fell English` from Google Fonts, loaded via `<link>` or bundled in `ui/static/fonts/`)

### Part D: Screenshot Replacement

20. **Implement screenshot capture via `tauri-plugin-screenshot` or JS canvas export**
    - Option A (preferred): use `window.__TAURI__.webviewWindow.current().capture()` (Tauri 2 API)
      to capture the webview as PNG, then save via `tauri-plugin-fs`
    - Implement `cargo run -- --screenshot docs/screenshots` CLI flag in `src-tauri/src/lib.rs`:
      launches the app, injects sample game state, waits 4 frames, captures at 4 time-of-day settings,
      saves PNGs, then quits
    - Update `CLAUDE.md` to use the new screenshot command

### Part E: Cleanup, Tests & Documentation

21. **Write unit tests for Tauri commands in `src-tauri/src/commands.rs`**
    - Test `get_world_snapshot` returns correctly structured data
    - Test `submit_input` with a movement command updates world state
    - Test `get_map` returns all locations with valid lat/lon
    - Use `tauri::test::mock_app()` (Tauri 2 testing utilities)

22. **Write frontend component tests**
    - Use Svelte Testing Library (`@testing-library/svelte`) + Vitest
    - Test `ChatPanel`: renders log entries, shows loading state, auto-scrolls
    - Test `MapPanel`: renders correct number of SVG nodes, highlights player location
    - Test `InputField`: submits on Enter, clears after submission, disables during streaming
    - Test `StatusBar`: displays correct time/weather/season from store

23. **Update `CLAUDE.md`**
    - Replace `cargo build` / `cargo run` instructions with Tauri equivalents:
      - Dev: `cargo tauri dev` (starts Vite dev server + Tauri app with hot-reload)
      - Build: `cargo tauri build` (production bundle)
      - Tests: `cargo test -p parish-core && cargo test -p parish-tauri && cd ui && npm test`
    - Update screenshot command
    - Update architecture tree to reflect new workspace layout

24. **Update `docs/design/overview.md`**
    - Replace `src/gui/` section with description of `src-tauri/` + `ui/` architecture
    - Update the architecture diagram to show Tauri IPC boundary between Rust and Svelte
    - Add note that the CSS variable theme system replaces the Rust palette application in egui

25. **Update `docs/adr/README.md`**
    - Add ADR-015 row to the index table

26. **Update `docs/requirements/roadmap.md`**
    - Add Phase 8 section with checkboxes for each task above
    - Note that Phase 7's egui-WASM target is superseded by the Svelte frontend

27. **Regenerate screenshots**
    - Run the new screenshot command and commit updated PNGs to `docs/screenshots/`

## Implementation Order

1. Tasks 1–3 (workspace + core extraction) — must complete first; all subsequent work depends on them
2. Task 4 (Tauri init) — unblocks backend work
3. Tasks 5–9 (Tauri backend) — can proceed in parallel with:
4. Tasks 10–12 (frontend scaffolding + stores) — can start as soon as `ui/` exists
5. Tasks 13–19 (UI components) — sequential within this group, Chat and StatusBar first as simplest
6. Task 20 (screenshots) — after UI components are stable
7. Tasks 21–22 (tests) — alongside components, not deferred to end
8. Tasks 23–27 (docs + cleanup) — final step before pushing

## Testing Strategy

**parish-core (Rust)**
- All existing unit tests must pass without modification after the extraction
- Run with `cargo test -p parish-core`

**src-tauri (Rust)**
- Command handler unit tests via `tauri::test::mock_app()`
- Streaming bridge: test that tokens accumulate and emit correctly with a mock `InferenceQueue`
- `cargo test -p parish-tauri`

**ui (TypeScript/Svelte)**
- Vitest + `@testing-library/svelte` for component tests
- Mock `@tauri-apps/api` in tests (standard Tauri testing pattern)
- `cd ui && npm test`

**Integration**
- Use the existing `GameTestHarness` (`--script` mode) via the CLI binary (TUI/headless) to verify
  game logic is unchanged after core extraction
- Visual verification: run `cargo tauri dev` and manually walk through a town → pub → NPC conversation

## Open Questions

1. **Token batching interval**: 16 ms is a starting guess. If text feels choppy, try 8 ms; if CPU
   usage is too high, try 32 ms. Make this a `const` in `events.rs` for easy tuning.
2. **Map projection**: Equirectangular is fine for a small parish area. If the OSM bounding box is
   very elongated, a Mercator projection may look better. Decide once the SVG map is first rendered.
3. **Fonts**: `IM Fell English` is a suggestion. The actual font choice should be validated against
   Irish fada characters (á é í ó ú) and legibility at small sizes in the chat log.
4. **Phase 7 alignment**: Phase 7 originally planned egui-WASM for the browser client. With Svelte
   on the frontend, the Phase 7 web client becomes a Svelte SPA connecting to the axum WebSocket
   server instead — a simpler and more capable approach. Phase 7 plan should be revised accordingly.
