# GUI Design

> Parent: [Architecture Overview](overview.md) | [Docs Index](../index.md)

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

The map renders the `WorldGraph` as a visual node-link diagram:

- **Nodes**: Circles positioned in a circular layout, labeled with location names
- **Edges**: Lines connecting neighboring locations
- **Player location**: Highlighted with accent color and thicker border
- **Adjacent locations**: Semi-highlighted, clickable to trigger movement
- **NPC markers**: Small dots above nodes showing NPC presence
- **Click-to-move**: Clicking an adjacent node calls `handle_movement()` directly

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
