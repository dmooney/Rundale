# Debug UI

> Parent: [Debug System](debug-system.md) | [Architecture Overview](overview.md) | [Docs Index](../index.md)

Comprehensive in-game debug UI for inspecting all game state, events, and internals. Renders in the Tauri/Svelte desktop GUI.

## Overview

The debug UI exposes a tabbed panel showing live game internals. It is toggled with **F12** or a **Debug** button in the StatusBar. All debug data flows through a `DebugSnapshot` struct in `parish-core`, consumed by the Tauri GUI frontend.

## Data Architecture

### `DebugSnapshot` (parish-core)

A single serializable struct aggregates all inspectable state:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct DebugSnapshot {
    pub clock: ClockDebug,
    pub world: WorldDebug,
    pub npcs: Vec<NpcDebug>,
    pub tier_summary: TierSummary,
    pub events: Vec<DebugEvent>,
    pub inference: InferenceDebug,
}
```

#### `ClockDebug`
- `game_time`: formatted datetime string
- `time_of_day`, `season`, `festival`: display strings
- `weather`: current weather
- `paused`: bool
- `speed_factor`: f64
- `real_elapsed_secs`: wall-clock seconds since game start

#### `WorldDebug`
- `player_location`: name + id
- `location_count`: total locations in graph
- `locations`: `Vec<LocationDebug>` — each with id, name, indoor/public, connection count, NPCs present (names)

#### `NpcDebug` (full deep-dive)
- `id`, `name`, `age`, `occupation`, `personality`
- `location_name`, `location_id`
- `home_name`, `workplace_name`
- `mood`, `state` (Present / InTransit with destination + ETA)
- `tier`: cognitive tier assignment
- `schedule`: `Vec<ScheduleEntryDebug>` — start_hour, end_hour, location_name, activity
- `relationships`: `Vec<RelationshipDebug>` — target_name, kind, strength
- `memories`: `Vec<MemoryDebug>` — timestamp, content, location_name
- `knowledge`: `Vec<String>`

#### `TierSummary`
- `tier1_count`, `tier2_count`, `tier3_count`, `tier4_count`
- `tier1_names`, `tier2_names`: Vec of NPC names

#### `InferenceDebug`
- `provider_name`, `model_name`, `base_url`
- `cloud_provider`, `cloud_model`: Option strings
- `has_queue`: bool (whether inference is configured)
- `improv_enabled`: bool

#### `DebugEvent`
- `timestamp`: formatted game time
- `category`: "schedule" | "tier" | "movement" | "encounter" | "system"
- `message`: human-readable description

### Builder Function

```rust
pub fn build_debug_snapshot(
    world: &WorldState,
    npc_manager: &NpcManager,
    events: &VecDeque<DebugEvent>,
    inference: &InferenceDebug,
) -> DebugSnapshot
```

Pure query function — no mutation, no allocation beyond the snapshot itself.

## Tauri GUI Debug Panel

### Component: `DebugPanel.svelte`

A collapsible bottom drawer that slides up from the bottom of the screen.

- **Toggle**: F12 key or a small debug icon button in the StatusBar
- **Height**: 40% of viewport when open, resizable
- **Tabs**: 5-tab horizontal tab bar (Overview, NPCs, World, Events, Inference)

### IPC

New Tauri command:

```rust
#[tauri::command]
pub async fn get_debug_snapshot(state: ...) -> Result<DebugSnapshot, String>
```

New Tauri event (emitted every 2 seconds when debug panel is open):

```
EVENT_DEBUG_UPDATE = "debug-update"
```

The frontend subscribes to `debug-update` only while the panel is visible (no overhead when closed).

### TypeScript Types

```typescript
interface DebugSnapshot {
    clock: ClockDebug;
    world: WorldDebug;
    npcs: NpcDebug[];
    tier_summary: TierSummary;
    events: DebugEvent[];
    inference: InferenceDebug;
}
```

All sub-interfaces follow the Rust struct field names (snake_case).

### Store

```typescript
// stores/debug.ts
export const debugVisible = writable<boolean>(false);
export const debugSnapshot = writable<DebugSnapshot | null>(null);
export const debugTab = writable<number>(0);
export const selectedNpcId = writable<number | null>(null);
```

## NPC Inspector (Deep-Dive)

The GUI supports selecting an individual NPC to see everything:

| Section | Data |
|---------|------|
| Identity | Name, age, occupation, personality |
| Location | Current (name), home, workplace |
| Status | Mood, cognitive tier, state (Present / InTransit → dest @ ETA) |
| Schedule | Hourly time blocks with location + activity |
| Relationships | Target name, kind, strength bar, history count |
| Memory | Last 10 entries: timestamp, content, location |
| Knowledge | Local gossip/facts list |

Clicking an NPC name in the NPC tab opens an inline expanded card.

## Event Log

A ring buffer (capacity 100) of `DebugEvent` entries, collected from:

- NPC schedule transitions (`tick_schedules` results)
- Tier reassignments
- Player movement
- Encounter rolls
- System messages (pause/resume/speed change)

Events are timestamped with game time and categorized for optional filtering.

## Implementation Files

| File | Purpose |
|------|---------|
| `crates/parish-core/src/debug_snapshot.rs` | `DebugSnapshot` + builder |
| `src/debug.rs` | Updated `/debug` commands to use snapshot |
| `src-tauri/src/commands.rs` | `get_debug_snapshot` command |
| `src-tauri/src/events.rs` | `EVENT_DEBUG_UPDATE` constant |
| `src-tauri/src/lib.rs` | Debug tick task (2s interval) |
| `ui/src/lib/types.ts` | TypeScript debug interfaces |
| `ui/src/lib/ipc.ts` | `getDebugSnapshot()` + `onDebugUpdate()` |
| `ui/src/stores/debug.ts` | Debug state store |
| `ui/src/components/DebugPanel.svelte` | Main debug panel component |

## Related

- [Debug System](debug-system.md) — Debug commands and feature gating
- [NPC System](npc-system.md) — NPC state model
- [Cognitive LOD](cognitive-lod.md) — Tier system
- [Inference Pipeline](inference-pipeline.md) — Provider configuration
