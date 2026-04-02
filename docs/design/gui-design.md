# GUI Design

> Parent: [Architecture Overview](overview.md) | [Docs Index](../index.md)

> **Note:** The desktop GUI has been migrated from egui to **Tauri 2 + Svelte 5**. See [Phase 8 plan](../plans/phase-8-tauri-gui.md) and [ADR 016](../adr/016-tauri-svelte-gui.md) for the Tauri architecture. The Svelte component source lives in `ui/src/components/`. The egui documentation below is retained for historical reference.

## Tauri / Svelte — Loading Spinner

The `ChatPanel.svelte` component displays an animated **Celtic triquetra (Trinity knot)** SVG while waiting for LLM responses (`$streamingActive === true`). The animation uses the `stroke-dasharray` / `stroke-dashoffset` CSS technique:

- **Three SVG arc paths** form the interlocking triquetra lobes, each with `pathLength="120"` for normalized dash control
- **Draw/erase cycle** (2.4s per lobe): `stroke-dashoffset` animates 120 → 0 → −120 to trace on, hold, then erase from the start
- **Staggered delays**: lobes draw sequentially at 0s, 0.8s, 1.6s offsets
- **Opacity pulse**: 0.3 → 1.0 → 0.3 adds depth to the draw cycle
- **Slow rotation**: the entire SVG rotates at 6s/revolution for organic motion
- **Theme-adaptive**: uses `var(--color-accent)` (gold) which changes with time-of-day palette

Once streaming tokens begin arriving, the spinner is replaced by a blinking cursor (`▋`) at the end of the streaming text. The streaming source label is derived from the last non-player, non-system log entry so the correct NPC name appears during token streaming.

The headless and TUI modes continue to use the Rust `LoadingAnimation` (`crates/parish-core/src/loading.rs`) with Celtic cross Unicode characters and Irish-themed phrases.

---

## Legacy egui GUI

## Overview

Parish has three UI modes, selectable via CLI flags:

| Mode | Flag | Framework | Description |
|------|------|-----------|-------------|
| **GUI** | *(default)* | egui + eframe | Windowed GUI with map, chat, and sidebars |
| **TUI** | `--tui` | Ratatui + Crossterm | Terminal UI with 24-bit true color |
| **Headless** | `--headless` | stdin/stdout | Plain REPL for testing and scripting |

The GUI mode provides an enhanced visual experience beyond what the terminal allows: an interactive location map, collapsible sidebars, and a proper windowed interface — while reusing all shared game logic.

## Architecture

The GUI is built on [egui](https://github.com/emilk/egui), an immediate-mode GUI library, via the [eframe](https://github.com/emilk/egui/tree/master/crates/eframe) framework (winit + wgpu backend). This gives us a cross-platform native window with GPU-accelerated rendering.

### Key Design Decision: Immediate Mode

egui redraws the entire UI every frame based on current state — no retained widget tree. This maps naturally to the game loop: each frame reads `WorldState`, `NpcManager`, and the text log, then renders panels. No synchronization between UI widgets and game state is needed.

### Async Bridge

The game uses Tokio for async inference calls, but egui runs a synchronous event loop. The bridge works the same way as the TUI:

```
Tokio Runtime (inference worker)
    ↓ tokens via mpsc::unbounded_channel
Token Accumulator Task (tokio::spawn)
    ↓ writes to Arc<Mutex<String>>
GuiApp::update() (egui frame)
    ↓ drains buffer into world.text_log
Panel Rendering
```

- `ctx.request_repaint()` is called when streaming is active (forces continuous redraw)
- `ctx.request_repaint_after(500ms)` provides periodic updates for the game clock
- The `tokio::runtime::Handle` is stored in `GuiApp` for spawning async tasks

## Layout

```
┌──────────────────────────────────────────────────────────────┐
│  Status Bar: Location | HH:MM TimeOfDay | Weather | Season  │
├──────────────────────────┬───────────────────────────────────┤
│                          │                                   │
│    Chat / Story Log      │         Map Panel                 │
│    (scrollable)          │    (interactive location graph)   │
│                          │                                   │
│                          ├───────────────────────────────────┤
│                          │    Focail — Words                 │
│                          │    (Irish pronunciation hints)    │
│                          │                                   │
│                          │    NPCs Here                      │
│                          │    (characters at location)       │
├──────────────────────────┴───────────────────────────────────┤
│  > Input field                                               │
└──────────────────────────────────────────────────────────────┘
```

### Panel Details

| Panel | egui Widget | Description |
|-------|-------------|-------------|
| **Status Bar** | `TopBottomPanel::top` | Location, game time, weather, season, festival, pause indicator |
| **Chat Panel** | `CentralPanel` | Scrollable text log with `stick_to_bottom(true)` auto-scroll |
| **Map Panel** | `SidePanel::right` (upper) | Location graph rendered via `Painter` API |
| **Sidebar** | `SidePanel::right` (lower) | Collapsible sections for Irish words and NPC info |
| **Input Field** | `TopBottomPanel::bottom` | Single-line `TextEdit` with Enter-to-submit |

### Map Panel

The map has two views: a **player-centered minimap** in the sidebar and a **full map overlay** triggered by `/map` command or `M` hotkey.

#### Minimap (MapPanel.svelte)

The sidebar minimap shows only the player's immediate neighborhood and smoothly pans to follow the player:

- **Player-centered viewport**: The SVG viewBox centers on the player's projected position, using Svelte `tweened` stores for smooth panning animation when the player moves
- **Hop filtering**: Only locations with `hops <= 1` (direct neighbors) are rendered as nodes; this keeps the view tightly zoomed with large, readable labels
- **Auto-zoom**: The viewBox dynamically fits the bounding box of visible locations with padding, capped at 2x the reference size to prevent labels from becoming too small
- **Off-screen indicators**: Locations beyond the viewport but within 3 hops are shown as small chevron arrows at the viewport boundary, pointing toward their direction
- **Nodes**: Circles positioned using a fixed-scale mercator projection (`map-projection.ts`), with grid fallback for locations without coordinates
- **Edges**: Lines connecting neighboring locations (only between visible nodes)
- **Player location**: Highlighted with accent color and larger radius
- **Adjacent locations**: Clickable to trigger movement
- **Expand button**: Opens the full map overlay (bordered icon with hover accent)
- **Label placement**: Greedy 8-position candidate algorithm (Imhof model) via `map-labels.ts`, with leader lines from node edge to displaced labels
- **M hotkey**: Toggles full map overlay, but only when the contenteditable input field is not focused (prevents stealing typed "m" characters)

#### Full Map Overlay (FullMapOverlay.svelte)

A modal overlay showing all parish locations with zoom and pan:

- **Triggered by**: `/map` command, `M` hotkey, or expand button on minimap
- **Zoom**: Mouse wheel zoom (0.5x–4x range) via CSS `scale()` transform
- **Pan**: Click-and-drag via pointer events
- **Close**: `Escape` key, `M` key toggle, backdrop click, or close button
- **Labels**: 11px font, 20-char truncation, placed using the Imhof 8-position candidate algorithm to avoid overlapping nodes and other labels
- **Leader lines**: Drawn from node edge toward label center using angle-based positioning (not fixed "below node" assumption)
- **Click-to-travel**: Adjacent locations remain clickable

#### Label Placement Algorithm (map-labels.ts)

Uses a **greedy 8-position candidate model** based on the Imhof cartographic convention (Christensen, Marks & Shieber 1995), followed by iterative force refinement:

1. **Phase 1 — Greedy candidate selection**: For each node, 8 candidate label positions are generated (NE, E, SE, S, SW, W, NW, N). Each is scored by overlap area with already-placed labels and all node circles, plus an Imhof preference penalty (NE preferred, SW least preferred) and out-of-bounds penalty. The lowest-penalty position is chosen greedily.
2. **Phase 2 — Force refinement**: 20 iterations of pairwise label-label push-apart resolve any remaining overlaps from imperfect greedy choices.
3. **Bounds clamping**: Labels are clamped to stay within the SVG viewBox.

The `estimateTextWidth(name, maxChars, fontSize)` function approximates SVG text width at ~0.6em per character, scaling with font size.

See [Map Evolution](map-evolution.md) for future map improvements (fog of war, animated travel, OSM tiles).

### Sidebar

Two collapsible sections (`CollapsingHeader`):

1. **Focail — Words**: Shows the 15 most recent `IrishWordHint` entries from NPC dialogue, each with the Irish word, phonetic pronunciation, and English meaning
2. **NPCs Here**: Lists all NPCs at the player's current location with name, occupation, and current mood

## Color System

The GUI uses smoothly interpolated time-of-day palettes with season and weather tinting, computed by the shared `src/world/palette.rs` engine and converted to `egui::Color32`. The 7 base keyframe palettes define colors at anchor hours, with linear interpolation between them for gradual transitions:

| Time | Background | Text | Accent |
|------|-----------|------|--------|
| Dawn | `(255,220,180)` warm pale | `(60,40,20)` dark brown | `(200,140,60)` gold |
| Morning | `(255,245,220)` warm gold | `(50,35,15)` dark brown | `(180,130,50)` amber |
| Midday | `(255,255,240)` bright warm | `(40,30,10)` near-black | `(160,120,40)` gold |
| Afternoon | `(240,220,170)` deep gold | `(50,35,15)` dark brown | `(180,130,50)` amber |
| Dusk | `(60,70,110)` deep blue | `(220,210,190)` light | `(200,160,80)` amber |
| Night | `(20,25,40)` near-black | `(180,180,190)` silver | `(100,110,140)` blue-grey |
| Midnight | `(10,12,20)` darkest | `(150,150,165)` muted | `(70,75,100)` dark blue |

The `GuiPalette` struct extends the TUI's 3-color palette to 7 colors, adding derived values for panels, inputs, borders, and muted text. Palettes are computed each frame by `compute_palette()` (from `src/world/palette.rs`), which smoothly interpolates between keyframes and applies season/weather tinting, then applied via `ctx.set_visuals()`. See [Weather System](weather-system.md) for tint parameters.

## GuiApp Struct

`GuiApp` mirrors the TUI's `App` struct but implements `eframe::App` instead of driving a terminal render loop:

```rust
pub struct GuiApp {
    // Shared game state (identical to TUI)
    pub world: WorldState,
    pub npc_manager: NpcManager,
    pub inference_queue: Option<InferenceQueue>,
    pub client: Option<OpenAiClient>,
    // ... provider config fields ...

    // GUI-specific state
    pub input_buffer: String,
    pub show_map: bool,
    pub show_sidebar: bool,
    pub show_debug: bool,

    // Async bridge
    pub tokio_handle: tokio::runtime::Handle,
    pub streaming_buf: Arc<Mutex<String>>,
    pub streaming_active: Arc<Mutex<bool>>,
}
```

The `eframe::App::update()` method runs each frame:

1. Drain streaming buffer → append to `world.text_log`
2. Check idle tick (20-second interval for NPC schedule simulation)
3. Compute smoothly interpolated palette (time + season + weather) and apply to `ctx.set_visuals()`
4. Render all panels
5. Process submitted input through `classify_input()` / game logic
6. Request repaint if streaming or after 500ms for clock updates

## Input Processing

Input follows the same pipeline as TUI and headless modes:

```
Text from input field
    ↓
classify_input() → SystemCommand or GameInput
    ↓
SystemCommand → handle_system_command() (pause, quit, provider, etc.)
GameInput → process_game_input()
    ├─ Local move detection ("go north", "walk to pub")
    ├─ Local look detection ("look", "look around")
    └─ NPC conversation → inference queue → token streaming
```

The GUI uses **synchronous local intent parsing** for movement and look commands (no LLM needed), keeping the UI responsive. NPC conversations are dispatched asynchronously via `tokio_handle.spawn()`.

## System Commands

All `/commands` work in GUI mode. The `/irish` and `/debug panel` commands toggle sidebar and debug panel visibility respectively.

## Entry Point

```
cargo run
```

The `run_gui()` function in `src/gui/mod.rs`:

1. Initializes the inference pipeline (tokio channel + worker)
2. Loads world data from `data/parish.json` and NPCs from `data/npcs.json`
3. Creates a `GuiApp` with the tokio runtime handle
4. Launches `eframe::run_native()` with a 1200x800 window (min 800x500)

## Window Properties

| Property | Value |
|----------|-------|
| Title | "Parish — An Irish Living World Text Adventure" |
| Default size | 1200 x 800 |
| Minimum size | 800 x 500 |
| Right panel width | 250–320px |
| Map height | 55% of right panel |

## Screenshots

Automated screenshot capture is built into the GUI via `--screenshot`:

```sh
xvfb-run -a cargo run -- --screenshot docs/screenshots
```

This opens the GUI in a virtual framebuffer, renders at 4 times of day (morning, midday, dusk, night), captures each as a 1200x800 PNG, and exits. Screenshots are saved to `docs/screenshots/`:

| File | Time of Day |
|------|-------------|
| `gui-morning.png` | 08:00 — warm gold palette |
| `gui-midday.png` | 12:00 — bright warm palette |
| `gui-dusk.png` | 17:00 — deep blue/amber palette |
| `gui-night.png` | 21:00 — near-black palette |

The capture uses egui's `ViewportCommand::Screenshot` API with `image` crate for PNG encoding. Sample game content (location descriptions, NPC dialogue, Irish word hints) is populated automatically so screenshots look representative.

**Screenshots must be regenerated any time `src/gui/` changes.** See `CLAUDE.md` for the command.

## Related

- [TUI Design](tui-design.md) — Terminal UI layout and color system (parallel implementation)
- [Time System](time-system.md) — Day/night cycle drives palette selection
- [Player Input](player-input.md) — Input parsing shared across all UI modes
- [Inference Pipeline](inference-pipeline.md) — Async LLM integration and token streaming

## Source Modules

- [`src/gui/mod.rs`](../../src/gui/mod.rs) — `GuiApp` struct, `eframe::App` impl, game loop
- [`src/gui/theme.rs`](../../src/gui/theme.rs) — Smooth time-of-day color palettes for egui
- [`src/world/palette.rs`](../../src/world/palette.rs) — Shared color interpolation engine
- [`src/gui/chat_panel.rs`](../../src/gui/chat_panel.rs) — Scrollable text log
- [`src/gui/map_panel.rs`](../../src/gui/map_panel.rs) — Interactive location graph
- [`src/gui/status_bar.rs`](../../src/gui/status_bar.rs) — Top status bar
- [`src/gui/sidebar.rs`](../../src/gui/sidebar.rs) — Irish words + NPC info
- [`src/gui/input_field.rs`](../../src/gui/input_field.rs) — Text input widget
- [`src/gui/screenshot.rs`](../../src/gui/screenshot.rs) — Automated screenshot capture
