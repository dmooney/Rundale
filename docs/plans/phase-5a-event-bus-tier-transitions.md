# Plan: Phase 5A — Event Bus & Tier Transitions

> Parent: [Phase 5](phase-5-full-lod-scale.md) | [Roadmap](../requirements/roadmap.md) | [Docs Index](../index.md)
>
> **Status: Planned**
>
> **Depends on:** Phase 4 (persistence)
> **Depended on by:** 5B (weather publishes events), 5C (gossip consumes events), 5D (Tier 3 publishes/consumes events), 5E (Tier 4 publishes events)

## Goal

Establish the cross-tier event bus and implement NPC state inflation/deflation so that tier transitions are seamless. This is foundational infrastructure that all other Phase 5 sub-phases depend on.

## Tasks

### 1. Event Bus (`crates/parish-core/src/world/events.rs` — new file)

Define the `WorldEvent` enum and `EventBus` struct.

```rust
use chrono::{DateTime, Utc};
use tokio::sync::broadcast;
use crate::npc::NpcId;
use crate::world::LocationId;

/// A world-level event that crosses tier boundaries.
#[derive(Debug, Clone)]
pub enum WorldEvent {
    /// An NPC said something notable (Tier 1 dialogue).
    DialogueOccurred {
        speaker: NpcId,
        location: LocationId,
        summary: String,
        timestamp: DateTime<Utc>,
    },
    /// An NPC's mood changed (any tier).
    MoodChanged {
        npc_id: NpcId,
        old_mood: String,
        new_mood: String,
    },
    /// A relationship shifted (any tier).
    RelationshipChanged {
        from: NpcId,
        to: NpcId,
        delta: f64,
    },
    /// An NPC arrived at a location.
    NpcArrived {
        npc_id: NpcId,
        location: LocationId,
    },
    /// An NPC departed a location.
    NpcDeparted {
        npc_id: NpcId,
        from: LocationId,
        to: LocationId,
    },
    /// Weather changed (added in Phase 5B).
    WeatherChanged {
        old: String,
        new: String,
    },
    /// A festival has begun (added in Phase 5E).
    FestivalStarted {
        name: String,
    },
    /// A Tier 4 life event occurred (added in Phase 5E).
    LifeEvent {
        description: String,
    },
}

/// Broadcast-based event bus for cross-tier communication.
///
/// All tier ticks subscribe via `subscribe()` and publish via `publish()`.
/// Uses `tokio::sync::broadcast` with a bounded buffer.
pub struct EventBus {
    sender: broadcast::Sender<WorldEvent>,
}

impl EventBus {
    /// Creates a new event bus with the given buffer capacity.
    pub fn new(capacity: usize) -> Self;

    /// Publishes an event to all subscribers. Silently drops if no subscribers.
    pub fn publish(&self, event: WorldEvent);

    /// Returns a new receiver for subscribing to events.
    pub fn subscribe(&self) -> broadcast::Receiver<WorldEvent>;
}
```

**Capacity**: 256 events. Events that overflow are dropped (lagging subscribers lose old events, which is acceptable for background tiers).

**Integration point**: `WorldState` gains an `event_bus: EventBus` field. The bus is created once at world initialization and shared via `&EventBus` references.

### 2. Tier Inflation — distant → close (`crates/parish-core/src/npc/transitions.rs` — new file)

When `NpcManager::assign_tiers` promotes an NPC from Tier 3/4 to Tier 1/2:

```rust
/// Builds a narrative context summary for an NPC being promoted to a
/// higher cognitive tier. Injected as a synthetic MemoryEntry.
pub fn inflate_npc_context(
    npc: &Npc,
    recent_events: &[WorldEvent],
) -> String
```

- Filter `recent_events` for events involving this NPC (by `npc_id`).
- Build a summary: "You are {name}. Recently, you've been {activity}. Your mood has been {mood}. {relationship_changes_narrative}."
- Add the summary as a synthetic `MemoryEntry` to the NPC's `ShortTermMemory`.

### 3. Tier Deflation — close → distant (`crates/parish-core/src/npc/transitions.rs`)

When an NPC is demoted from Tier 1/2 to Tier 3/4:

```rust
/// Compacts an NPC's short-term memory into a summary for lower-tier use.
pub struct NpcSummary {
    pub npc_id: NpcId,
    pub location: LocationId,
    pub mood: String,
    pub recent_activity: String,
    pub key_relationship_changes: Vec<(NpcId, f64)>,
}

pub fn deflate_npc_state(npc: &Npc) -> NpcSummary
```

- Extract mood, location, and the most recent 3 memory entries into `recent_activity`.
- Store the `NpcSummary` on the NPC (new `deflated_summary: Option<NpcSummary>` field on `Npc`).

### 4. Wire transitions into `NpcManager::assign_tiers`

- Track the previous tier assignment for each NPC.
- When a tier changes: call `inflate_npc_context` on promotion, `deflate_npc_state` on demotion.
- Publish `WorldEvent::NpcArrived` / `WorldEvent::NpcDeparted` as appropriate.

### 5. Event journal bridge

- Subscribe to the event bus from the persistence layer (Phase 4).
- Append relevant `WorldEvent`s to the SQLite journal as structured entries.
- This keeps the save system aware of cross-tier state changes.

## Tests

| Test | What it verifies |
|------|------------------|
| `test_event_bus_publish_subscribe` | Subscriber receives published events |
| `test_event_bus_multiple_subscribers` | All subscribers get all events |
| `test_event_bus_no_subscribers_no_panic` | Publishing with no subscribers succeeds silently |
| `test_inflate_produces_context` | Inflate builds a non-empty summary including NPC name and recent activity |
| `test_inflate_injects_memory` | Inflated summary appears as a MemoryEntry in short-term memory |
| `test_deflate_captures_state` | Deflate captures mood, location, and recent memories |
| `test_tier_transition_fires_on_promotion` | Moving player toward distant NPC triggers inflate |
| `test_tier_transition_fires_on_demotion` | Moving player away triggers deflate |

## Acceptance Criteria

- `EventBus` compiles and passes all tests
- Tier transitions produce observable state changes (memory injection, summary creation)
- Events published in one tier tick are receivable by other tier subscribers
- No panics when bus has no subscribers or buffer overflows
