# GUI Design

> Parent: [Architecture Overview](overview.md) | [Docs Index](../index.md)

## Loading Spinner

The `ChatPanel.svelte` component displays an animated **Celtic triquetra (Trinity knot)** SVG while waiting for LLM responses (`$streamingActive === true`). The animation uses the `stroke-dasharray` / `stroke-dashoffset` CSS technique:

- **Three SVG arc paths** form the interlocking triquetra lobes, each with `pathLength="120"` for normalized dash control
- **Draw/erase cycle** (2.4s per lobe): `stroke-dashoffset` animates 120 → 0 → −120 to trace on, hold, then erase from the start
- **Staggered delays**: lobes draw sequentially at 0s, 0.8s, 1.6s offsets
- **Opacity pulse**: 0.3 → 1.0 → 0.3 adds depth to the draw cycle
- **Slow rotation**: the entire SVG rotates at 6s/revolution for organic motion
- **Theme-adaptive**: uses `var(--color-accent)` (gold) which changes with time-of-day palette

Once streaming tokens begin arriving, the spinner is replaced by a blinking cursor (`▋`) at the end of the streaming text. The streaming source label is derived from the last non-player, non-system log entry so the correct NPC name appears during token streaming.

Headless mode continues to use the Rust `LoadingAnimation` (`crates/parish-core/src/loading.rs`) with Celtic cross Unicode characters and Irish-themed phrases.

---

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

Two collapsible sections:

1. **Focail — Words**: Shows the 15 most recent `IrishWordHint` entries from NPC dialogue, each with the Irish word, phonetic pronunciation, and English meaning
2. **NPCs Here**: Lists all NPCs at the player's current location with name, occupation, and current mood

## Color System

The GUI uses time-of-day palettes with season and weather tinting, computed by the shared `src/world/palette.rs` engine. The 7 defined palettes cover the major times of day:

| Time | Background | Text | Accent |
|------|-----------|------|--------|
| Dawn | `(255,220,180)` warm pale | `(60,40,20)` dark brown | `(200,140,60)` gold |
| Morning | `(255,245,220)` warm gold | `(50,35,15)` dark brown | `(180,130,50)` amber |
| Midday | `(255,255,240)` bright warm | `(40,30,10)` near-black | `(160,120,40)` gold |
| Afternoon | `(240,220,170)` deep gold | `(50,35,15)` dark brown | `(180,130,50)` amber |
| Dusk | `(60,70,110)` deep blue | `(220,210,190)` light | `(200,160,80)` amber |
| Night | `(20,25,40)` near-black | `(180,180,190)` silver | `(100,110,140)` blue-grey |
| Midnight | `(10,12,20)` darkest | `(150,150,165)` muted | `(70,75,100)` dark blue |

Palettes are selected by `compute_palette()` (from `src/world/palette.rs`) and season/weather tinting is applied on top. See [Weather System](weather-system.md) for tint parameters.

## Input Processing

Input follows the same pipeline as headless mode:

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

The GUI uses **synchronous local intent parsing** for movement and look commands (no LLM needed), keeping the UI responsive. NPC conversations are dispatched asynchronously.

## System Commands

All `/commands` work in GUI mode. The `/irish` and `/debug panel` commands toggle sidebar and debug panel visibility respectively.

## Related

- [Time System](time-system.md) — Day/night cycle drives palette selection
- [Player Input](player-input.md) — Input parsing shared across all UI modes
- [Inference Pipeline](inference-pipeline.md) — Async LLM integration and token streaming
- [ADR 016](../adr/016-tauri-svelte-gui.md) — Tauri 2 + Svelte 5 architecture decision

## Source Modules

- [`apps/ui/src/components/`](../../apps/ui/src/components/) — Svelte UI components (ChatPanel, MapPanel, FullMapOverlay, Sidebar, StatusBar)
- [`apps/ui/src/lib/map-labels.ts`](../../apps/ui/src/lib/map-labels.ts) — Label placement algorithm
- [`apps/ui/src/lib/map-projection.ts`](../../apps/ui/src/lib/map-projection.ts) — Mercator projection
- [`src/world/palette.rs`](../../src/world/palette.rs) — Time-of-day palette engine
