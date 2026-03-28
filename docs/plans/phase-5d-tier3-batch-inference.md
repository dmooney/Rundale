# Plan: Phase 5D — Tier 3 Batch Inference

> Parent: [Phase 5](phase-5-full-lod-scale.md) | [Roadmap](../requirements/roadmap.md) | [Docs Index](../index.md)
>
> **Status: Planned**
>
> **Depends on:** Phase 5A (event bus), Phase 5C (long-term memory for context)
> **Depended on by:** 5E (Tier 4 depends on Tier 3 being operational for priority queue)

## Goal

Implement Tier 3 cognitive simulation: a single LLM call that bulk-simulates many distant NPCs at once, producing daily activity summaries, mood changes, and relationship shifts.

## Tasks

### 1. `Tier3Update` struct (`crates/parish-core/src/npc/types.rs`)

Add to the existing types file:

```rust
/// The result of a Tier 3 batch simulation for a single NPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tier3Update {
    /// Which NPC this update is for.
    pub npc_id: NpcId,
    /// New location (if NPC moved).
    #[serde(default)]
    pub new_location: Option<LocationId>,
    /// Updated mood string.
    #[serde(default)]
    pub mood: String,
    /// Summary of what the NPC did during the simulated period.
    #[serde(default)]
    pub activity_summary: String,
    /// Relationship changes: (other_npc_id, strength_delta).
    #[serde(default)]
    pub relationship_changes: Vec<RelationshipChange>,
}

/// The full response from a Tier 3 batch LLM call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tier3Response {
    pub updates: Vec<Tier3Update>,
}
```

### 2. Tier 3 tick function (`crates/parish-core/src/npc/ticks.rs`)

```rust
/// Runs a Tier 3 batch simulation for distant NPCs.
///
/// Builds a single prompt summarizing all Tier 3 NPCs and their states,
/// sends it to the simulation LLM client, parses the JSON array response,
/// and distributes updates to individual NPCs.
pub async fn tick_tier3(
    npcs: &[&Npc],
    world: &WorldState,
    client: &OpenAiClient,
    model: &str,
) -> Result<Vec<Tier3Update>, ParishError>
```

**Prompt template:**

```
System: You are simulating background NPC activity in a rural Irish parish in 1820.
Given the following NPCs and their current states, simulate {hours} hours of activity.
The weather is {weather}. The season is {season}. The time is {time_of_day}.

Return a JSON object with an "updates" array. Each update has:
- npc_id (integer)
- mood (string, one word)
- activity_summary (string, 1 sentence)
- new_location (integer or null)
- relationship_changes (array of {from, to, delta})

NPCs:
{npc_summaries}
```

**NPC summary format** (~100-150 tokens each):

```
NPC {id} "{name}" ({occupation}, age {age}): At {location}. Mood: {mood}.
Recent: {deflated_summary or last activity}.
Relationships: {name1} ({strength}), {name2} ({strength}).
```

### 3. Batching logic

- **Batch size**: configurable constant `TIER3_BATCH_SIZE = 10` (default).
- If more than `TIER3_BATCH_SIZE` Tier 3 NPCs exist, split into multiple batches.
- Process batches sequentially (one LLM call at a time to avoid overloading local inference).
- Use the simulation client (`InferenceClients::simulation_client()`).
- Model: 8B or 3B (selected via `config.rs` or auto-detected by VRAM).

### 4. Tick scheduling in `NpcManager`

```rust
impl NpcManager {
    /// Game time of the last Tier 3 tick.
    last_tier3_game_time: Option<DateTime<Utc>>,

    /// Returns whether enough game time has elapsed for a Tier 3 tick.
    /// Tier 3 ticks every 1 in-game day (~20 real seconds at default speed).
    pub fn needs_tier3_tick(&self, current_game_time: DateTime<Utc>) -> bool;

    /// Records that a Tier 3 tick was performed.
    pub fn record_tier3_tick(&mut self, time: DateTime<Utc>);

    /// Returns all NPCs assigned to Tier 3.
    pub fn tier3_npcs(&self) -> Vec<NpcId>;
}
```

### 5. Apply updates

After receiving `Vec<Tier3Update>`:

- For each update, find the NPC by `npc_id`.
- Apply `mood`, `activity_summary` (store on NPC as `last_activity: String`).
- If `new_location` is `Some`, update `npc.location` (validate it exists in graph).
- Apply `relationship_changes` via `Relationship::adjust_strength()`.
- Publish relevant `WorldEvent`s via the event bus.

### 6. Priority queue enforcement

Modify `InferenceQueue` or the tick dispatch logic:

- Tier 1/2 requests always preempt Tier 3 in the queue.
- If a Tier 3 batch is in-flight and a Tier 1 request arrives, the Tier 3 response is still processed (we can't cancel HTTP), but the next Tier 3 batch is deferred.
- If a Tier 3 tick can't complete before the next one is due, it is **skipped** (not queued).

```rust
/// Priority levels for inference requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InferencePriority {
    /// Tier 1: player-facing dialogue (highest).
    Interactive = 0,
    /// Tier 2: nearby NPC background simulation.
    Background = 1,
    /// Tier 3: distant NPC batch simulation (lowest LLM priority).
    Batch = 2,
}
```

### 7. Add `last_activity` field to `Npc`

```rust
pub struct Npc {
    // ... existing fields ...
    /// Last activity summary from Tier 3 simulation (used in deflated context).
    pub last_activity: Option<String>,
}
```

### 8. Assign Tier 3 vs Tier 4

Update `NpcManager::assign_tiers()` to distinguish Tier 3 from Tier 4:

- Distance 0: Tier 1
- Distance 1-2: Tier 2
- Distance 3-5: Tier 3
- Distance 6+: Tier 4

(Currently distance 3+ is all Tier 3; need to split.)

## Tests

| Test | What it verifies |
|------|------------------|
| `test_tier3_response_parsing` | Mock JSON response parses into correct `Tier3Update` structs |
| `test_tier3_response_partial` | Handles missing optional fields gracefully |
| `test_tier3_prompt_construction` | Prompt includes all NPC summaries and world context |
| `test_tier3_batching` | 25 NPCs split into 3 batches of 10, 10, 5 |
| `test_tier3_update_application` | Mood, location, relationships updated correctly |
| `test_tier3_invalid_location_ignored` | Update with nonexistent location_id is skipped |
| `test_tier3_tick_interval` | Tick fires every in-game day, not before |
| `test_tier3_skip_on_overdue` | If tick can't complete in time, next one is skipped |
| `test_priority_interactive_over_batch` | Interactive request is processed before batch |
| `test_tier_assignment_3_vs_4` | Distance 3-5 = Tier 3, distance 6+ = Tier 4 |

## Acceptance Criteria

- Tier 3 NPCs receive daily batch updates from LLM
- Updates are distributed correctly to individual NPCs
- Batch size is configurable and splits correctly
- Priority queue ensures player-facing inference is never delayed by Tier 3
- Tier 3 ticks are skipped (not queued) if they fall behind
- All tests passing
