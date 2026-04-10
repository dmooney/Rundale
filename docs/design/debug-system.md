# Debug System

> Parent: [Architecture Overview](overview.md) | [Docs Index](../index.md)

The debug system provides runtime visibility into Rundale's background simulation. It exposes NPC state, inference pipeline metrics, background task health, and performance data through slash commands.

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

- **Inference latency**: p50, p95, p99 over rolling window
- **Memory usage**: RSS, heap (if available)
- **Queue wait time**: average time requests spend in the inference queue

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

## Related

- [Player Input](player-input.md) — Debug commands extend the `/` command system
- [Inference Pipeline](inference-pipeline.md) — Metrics source for inference debug
- [Cognitive LOD](cognitive-lod.md) — Tier assignments displayed in debug
- [NPC System](npc-system.md) — NPC state exposed through debug commands

## Source Modules

- [`src/debug.rs`](../../src/debug.rs) — Debug commands and metrics
- [`src/input/`](../../src/input/) — Debug command parsing
- [`src/npc/`](../../src/npc/) — NPC state access for debug views
- [`src/inference/`](../../src/inference/) — Inference metrics collection
