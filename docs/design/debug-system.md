# Debug System

> Parent: [Architecture Overview](overview.md) | [Docs Index](../index.md)

The debug system provides runtime visibility into Parish's background simulation. It exposes NPC state, inference pipeline metrics, background task health, and performance data through both slash commands and a live TUI panel.

All debug functionality is gated behind a `debug` cargo feature flag and compiles out of release builds entirely.

## Feature Gating

```toml
# Cargo.toml
[features]
debug = []
```

All debug code is behind `#[cfg(feature = "debug")]`:

- Debug command parsing and dispatch
- `DebugState` struct and metrics collection
- Debug panel TUI widget
- Ring buffer log capture
- Inference request preview storage

**Dev builds**: `cargo build --features debug`
**Release builds**: `cargo build --release` (zero debug overhead)

## Debug Commands

Debug commands extend the existing `/` command system. All prefixed with `/debug`.

| Command | Description |
|---------|-------------|
| `/debug npcs` | List all NPCs: name, location, mood, current activity, tier assignment |
| `/debug npc <name\|id>` | Full state dump for a single NPC (see below) |
| `/debug inference` | Inference queue status with recent request previews |
| `/debug tiers` | NPC tier assignments: count per tier, NPC names, last tick time |
| `/debug world` | Game clock, real elapsed time, speed factor, weather, season, player location, total NPC count |
| `/debug tasks` | Background task health: status, last activity, error counts |
| `/debug log [subsystem]` | Recent tracing log entries, optionally filtered by subsystem |
| `/debug perf` | Performance metrics: frame time, inference latency percentiles, memory usage |
| `/debug panel` | Toggle the live debug panel overlay |

### `/debug npc <name|id>` — Full NPC State Dump

Displays all state for a single NPC:

- **Identity**: name, age, occupation, personality traits
- **Location**: current node, home node, workplace node
- **Schedule**: current schedule entry, next transition
- **Mood**: current emotional state
- **Tier**: current cognitive tier assignment
- **Relationships**: list with target name, kind, strength
- **Memory**: recent short-term memory entries (last 5)
- **Knowledge**: current knowledge entries
- **Last action**: most recent `NpcAction` output from inference

### `/debug inference` — Queue & Throughput

Displays:

- **Queue depth**: pending requests waiting for inference
- **Active**: currently processing request (model, NPC, elapsed time)
- **Throughput**: requests/sec and tokens/sec (rolling 30-second window)
- **Model assignment**: which model is assigned to each tier
- **Recent requests** (last 10): truncated prompt (first 200 chars) and response (first 200 chars), with NPC name, tier, model, and latency

### `/debug tasks` — Background Task Health

Lists all background async tasks with health indicators:

| Task | Tracked Metrics |
|------|-----------------|
| Inference worker | Status (idle/busy), queue depth, requests processed, errors, last activity |
| Autosave | Last save time, next scheduled save, save duration, errors |
| Tier 2 simulation | Last tick time, NPCs processed, tick duration |
| Tier 3 batch | Last batch time, NPCs processed, batch duration |
| Tier 4 rules engine | Last tick time, events generated |

Health states:

- **Healthy** (green): active within expected interval, no recent errors
- **Warning** (yellow): behind schedule or isolated errors
- **Error** (red): repeated failures or unresponsive

### `/debug log [subsystem]` — Tracing Log Viewer

Shows recent tracing log entries captured in a ring buffer (last 200 entries). Optional subsystem filter:

- `inference` — Ollama requests, responses, errors
- `npc` — NPC state changes, tier transitions, action results
- `persistence` — Save/load operations, journal writes
- `world` — Clock ticks, weather changes, player movement

Example: `/debug log inference` shows only inference-related log lines.

### `/debug perf` — Performance Metrics

- **Frame time**: TUI render duration (ms), target vs actual
- **Inference latency**: p50, p95, p99 over rolling window
- **Memory usage**: RSS, heap (if available)
- **Queue wait time**: average time requests spend in the inference queue

## Debug Panel (TUI Overlay)

A live-updating panel that coexists with the main game view. Toggled via `/debug panel` or the `F12` key.

### Layout

When active, the debug panel splits the terminal layout:

```
┌─────────────────────────────────┬──────────────────────┐
│ Top bar: location | time | wx   │                      │
├─────────────────────────────────┤   Debug Panel        │
│                                 │                      │
│   Main text panel               │   [Overview] [NPCs]  │
│   (game output)                 │   [Inference] [Tasks] │
│                                 │                      │
│                                 │   (tab content)      │
│                                 │                      │
├─────────────────────────────────┤                      │
│ > player input                  │                      │
└─────────────────────────────────┴──────────────────────┘
```

The debug panel takes roughly 35% of terminal width. On narrow terminals (< 120 cols), it renders as a bottom split instead.

### Tabs

**Overview** — At-a-glance dashboard:
- Game clock and real elapsed time
- Weather and season
- NPC count by tier (e.g., `T1: 3  T2: 12  T3: 47  T4: 230`)
- Inference queue depth with mini sparkline
- Background task health indicators (colored dots)

**NPCs** — Scrollable NPC list:
- Columns: name, location, mood, activity, tier
- Search/filter by name or location (type to filter)
- Select an NPC to expand inline state detail
- Sorted by tier (T1 first), then alphabetically

**Inference** — Pipeline monitor:
- Queue depth sparkline (last 60 seconds)
- Throughput counter (req/sec, tokens/sec)
- Active request: NPC name, model, elapsed time
- Recent requests table: NPC, tier, model, latency, truncated prompt/response (first 200 chars each)

**Tasks** — Background task monitor:
- Each task as a row: name, status indicator (colored), uptime, last activity timestamp, error count
- Expandable detail: recent errors, performance history

### Navigation

- `Tab` / `Shift+Tab`: cycle between debug panel tabs
- Arrow keys: scroll within the active tab
- `/` in NPC tab: activate search filter
- `F12`: close the panel

## Debug Data Architecture

### `DebugState` Struct

Central aggregation point for all debug metrics:

```rust
#[cfg(feature = "debug")]
pub struct DebugState {
    /// NPC state snapshots (refreshed on access)
    pub npc_summaries: Vec<NpcDebugSummary>,
    /// Inference pipeline metrics
    pub inference_metrics: InferenceMetrics,
    /// Background task health
    pub task_health: Vec<TaskHealth>,
    /// Recent log entries (ring buffer)
    pub log_buffer: RingBuffer<LogEntry>,
    /// Performance counters
    pub perf_metrics: PerfMetrics,
    /// Whether debug panel is visible
    pub panel_visible: bool,
    /// Active panel tab
    pub active_tab: DebugTab,
}
```

### Metrics Collection

Each subsystem publishes metrics through lightweight, bounded channels:

```
InferenceWorker ──→ metrics_tx ──→ DebugState
AutosaveTask ─────→ metrics_tx ──→ DebugState
TierSimulation ───→ metrics_tx ──→ DebugState
NpcManager ───────→ metrics_tx ──→ DebugState
```

- Channels are `tokio::sync::watch` for latest-value semantics (no backpressure, no queue buildup)
- When the debug feature is disabled, metric publish calls compile to no-ops
- When the debug panel is hidden, `DebugState` still collects (cheap) but skips rendering

### Inference Request Preview

Recent inference requests are stored in a bounded ring buffer (capacity 50):

```rust
#[cfg(feature = "debug")]
pub struct InferencePreview {
    pub npc_name: String,
    pub tier: CogTier,
    pub model: String,
    pub prompt_preview: String,    // first 200 chars
    pub response_preview: String,  // first 200 chars
    pub latency: Duration,
    pub timestamp: Instant,
}
```

Truncation happens at capture time — full prompts are never stored in the debug system.

### Tracing Integration

A custom tracing subscriber layer captures log entries into a ring buffer:

- Ring buffer capacity: 200 entries
- Each entry stores: timestamp, level, target (subsystem), message
- The layer is only installed when the `debug` feature is enabled
- `/debug log` reads from this buffer; the debug panel Tasks tab shows recent errors from it

## Implementation Considerations

- **Performance**: All debug collection uses `watch` channels and ring buffers — bounded memory, no allocations on the hot path when panel is hidden
- **Thread safety**: `DebugState` lives behind `Arc<RwLock<>>`, read-locked only during panel render (once per frame)
- **Compile-time elimination**: Every debug struct, channel, and render path is behind `#[cfg(feature = "debug")]` — zero cost in release builds
- **Existing integration**: Debug commands are parsed in the same `parse_system_command` path as `/save`, `/quit`, etc. — just additional match arms behind `#[cfg(feature = "debug")]`

## GUI Debug Inspector Panel

The GUI mode (egui/eframe) includes a floating debug inspector window with tabbed views for deep inspection of all game state. Toggled via `F12` or `/debug panel`.

### Layout

A resizable, moveable `egui::Window` overlaid on the game UI:

```
┌──────────────────────────────────────────────────────┐
│ Debug Inspector                               [X]    │
├───────┬───────────┬───────┬─────────┬───────────────┤
│ World │ Locations │ NPCs  │ Events  │ Relationships │
├───────┴───────────┴───────┴─────────┴───────────────┤
│                                                      │
│  [Tab content]                                       │
│                                                      │
└──────────────────────────────────────────────────────┘
```

### Tabs

**World** — At-a-glance dashboard:
- Game time, date, season, festival
- Weather, game speed, pause state
- Player location
- NPC tier distribution (Tier 1/2/3+ counts)

**Locations** — Two-pane location browser:
- Left: filterable list of all locations (player location marked with `*`)
- Right: selected location detail — name, indoor/public flags, description template, connections with travel times, associated NPCs, NPCs currently present, mythological significance

**NPCs** — Two-pane NPC browser:
- Left: filterable list showing name, mood, location, tier, transit state
- Right: full NPC detail with collapsible sections:
  - Identity (ID, age, occupation, mood, location, tier, state, home, workplace)
  - Personality (full prompt text)
  - Schedule (table of time slots with activity and location)
  - Memory (timestamped entries with location)
  - Relationships (grid with kind, strength, and visual strength bar)
  - Knowledge (list of known facts)

**Events** — Scrollable event log:
- Structured events with game-time timestamps and color-coded categories
- Categories: MOVE, SCHED, CHAT, TIER, MOOD, REL, HEAR, SYS
- Auto-scroll toggle
- 500-event ring buffer

**Relationships** — All NPC-to-NPC relationships:
- Filterable by NPC name
- Grid: From, To, Kind, Strength, visual strength bar (-1.0 to +1.0)

### Event Instrumentation

The debug event log captures:
- **Movement**: Player travel between locations (origin, destination, travel time)
- **Schedule**: NPC departures and arrivals from schedule ticking
- **Conversation**: Player-NPC dialogue initiation
- **Tier changes**: NPC cognitive tier reassignments
- Future phases will add: mood changes, relationship changes, overheard events

### Data Types

```rust
pub struct DebugEvent {
    pub timestamp: String,           // Game time HH:MM
    pub category: DebugEventCategory,
    pub message: String,
}

pub struct DebugUiState {
    pub active_tab: DebugTab,
    pub selected_location: Option<LocationId>,
    pub selected_npc: Option<NpcId>,
    pub npc_filter: String,
    pub location_filter: String,
    pub event_log_auto_scroll: bool,
    pub relationship_filter: String,
}
```

## Related

- [Player Input](player-input.md) — Debug commands extend the `/` command system
- [TUI Design](tui-design.md) — Debug panel layout within the TUI
- [Inference Pipeline](inference-pipeline.md) — Metrics source for inference debug
- [Cognitive LOD](cognitive-lod.md) — Tier assignments displayed in debug
- [NPC System](npc-system.md) — NPC state exposed through debug commands

## Source Modules

- [`src/gui/debug_panel.rs`](../../src/gui/debug_panel.rs) — GUI debug inspector window
- [`src/tui/`](../../src/tui/) — Debug panel widget, panel layout split
- [`src/input/`](../../src/input/) — Debug command parsing
- [`src/npc/`](../../src/npc/) — NPC state access for debug views
- [`src/inference/`](../../src/inference/) — Inference metrics collection
