# Plan: Phase 5C — NPC Long-Term Memory & Gossip Propagation

> Parent: [Phase 5](phase-5-full-lod-scale.md) | [Roadmap](../requirements/roadmap.md) | [Docs Index](../index.md)
>
> **Status: Planned**
>
> **Depends on:** Phase 5A (event bus for gossip-triggering events)
> **Depended on by:** 5D (Tier 3 uses long-term memory context), 5E (Tier 4 life events become gossip)

## Goal

Give NPCs persistent long-term memory with keyword-based retrieval, and implement a gossip propagation system where information spreads organically between NPCs during interactions.

## Tasks

### 1. Long-Term Memory (`crates/parish-core/src/npc/memory.rs` — extend existing file)

Extend the existing `memory.rs` which already contains `ShortTermMemory`:

```rust
/// A long-term memory entry with importance scoring and keyword tagging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongTermEntry {
    /// When this was originally experienced.
    pub timestamp: DateTime<Utc>,
    /// What happened.
    pub content: String,
    /// Importance score from 0.0 (trivial) to 1.0 (life-changing).
    pub importance: f32,
    /// Keywords for retrieval (NPC names, locations, event types).
    pub keywords: Vec<String>,
}

/// Long-term memory store with keyword-based retrieval.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LongTermMemory {
    entries: Vec<LongTermEntry>,
}

impl LongTermMemory {
    pub fn new() -> Self;

    /// Stores an entry if it meets the importance threshold.
    pub fn store(&mut self, entry: LongTermEntry);

    /// Retrieves the top `limit` entries matching the query by keyword overlap.
    ///
    /// Scoring: count of query keywords that appear in entry keywords,
    /// weighted by importance. Higher scores first.
    pub fn recall(&self, query_keywords: &[&str], limit: usize) -> Vec<&LongTermEntry>;

    /// Returns the total number of stored entries.
    pub fn len(&self) -> usize;

    /// Returns true if empty.
    pub fn is_empty(&self) -> bool;
}
```

**Keyword extraction**: When promoting from short-term memory, extract keywords by:
- NPC names mentioned in `participants`
- Location name from `location`
- Simple word extraction: nouns/verbs over 4 characters from `content`

**Importance scoring heuristic** (no LLM needed):
- Player involved: +0.3
- Multiple participants: +0.1
- Contains relationship change: +0.2
- Contains strong emotion words ("angry", "love", "death", "secret"): +0.2
- Base: 0.2

**Promotion threshold**: importance > 0.5

### 2. Short-term → Long-term promotion

Modify `ShortTermMemory::add()`:

```rust
/// Adds a new memory entry, returning the evicted entry (if any)
/// for potential long-term storage.
pub fn add(&mut self, entry: MemoryEntry) -> Option<MemoryEntry>
```

Change the return type to `Option<MemoryEntry>` so the caller can score and promote evicted entries. The `NpcManager` or tick function handles the promotion logic.

### 3. Long-term memory in Tier 1 context

Modify Tier 1 context construction in `npc/ticks.rs`:

- Extract keywords from the current conversation context (player input, NPC name, location name).
- Call `npc.long_term_memory.recall(keywords, 3)` to get the top 3 relevant memories.
- Inject into the system prompt: "You recall: {memory1}. {memory2}. {memory3}."

### 4. Add `long_term_memory` field to `Npc`

```rust
pub struct Npc {
    // ... existing fields ...
    /// Long-term memory for keyword-based recall.
    pub long_term_memory: LongTermMemory,
}
```

### 5. Gossip Network (`crates/parish-core/src/npc/gossip.rs` — new file)

```rust
use std::collections::HashSet;
use chrono::{DateTime, Utc};
use crate::npc::NpcId;

/// A piece of gossip circulating among NPCs.
#[derive(Debug, Clone)]
pub struct GossipItem {
    /// Unique id for deduplication.
    pub id: u32,
    /// Current content (may be distorted from original).
    pub content: String,
    /// Original source NPC.
    pub source: NpcId,
    /// Set of NPCs who know this gossip.
    pub known_by: HashSet<NpcId>,
    /// How many times this has been distorted (0 = original).
    pub distortion_level: u8,
    /// When the gossip originated.
    pub timestamp: DateTime<Utc>,
}

/// Manages all gossip items in the world.
pub struct GossipNetwork {
    items: Vec<GossipItem>,
    next_id: u32,
}

impl GossipNetwork {
    pub fn new() -> Self;

    /// Creates a new gossip item from a notable event.
    pub fn create(
        &mut self,
        content: String,
        source: NpcId,
        timestamp: DateTime<Utc>,
    ) -> u32;

    /// Attempts to propagate gossip between two interacting NPCs.
    ///
    /// For each gossip item known by `speaker` but not `listener`:
    /// - 60% chance of transmission
    /// - 20% chance of distortion on transmission
    ///
    /// Returns a list of gossip items that were transmitted (for
    /// injection into dialogue context).
    pub fn propagate(
        &mut self,
        speaker: NpcId,
        listener: NpcId,
        rng: &mut impl Rng,
    ) -> Vec<&GossipItem>;

    /// Returns all gossip items known by the given NPC.
    pub fn known_by(&self, npc_id: NpcId) -> Vec<&GossipItem>;

    /// Returns gossip items created after the given timestamp.
    pub fn recent(&self, since: DateTime<Utc>) -> Vec<&GossipItem>;
}
```

**Distortion rules** (simple string mutation, no LLM):

| Rule | Example |
|------|---------|
| Drop an adjective | "the angry farmer" → "the farmer" |
| Exaggerate quantity | "a few sheep" → "many sheep" |
| Shift emotional tone | "was upset" → "was furious" |
| Swap a name (5% chance) | "Padraig told" → "Tommy told" |

Implementation: regex-based replacements with a small dictionary of adjectives, quantities, and emotion words.

### 6. Gossip in Tier 1 dialogue

When constructing Tier 1 context:

- Call `gossip_network.known_by(npc_id)` to get gossip this NPC knows.
- Select the most recent 2 items.
- Inject: "You've heard that: {gossip1}. {gossip2}."

### 7. Gossip creation from events

Subscribe to the event bus:

- `WorldEvent::DialogueOccurred` with notable content → create gossip
- `WorldEvent::RelationshipChanged` with large delta (>0.3) → create gossip
- `WorldEvent::LifeEvent` → create gossip (Phase 5E)

### 8. Wire into `WorldState`

Add `gossip_network: GossipNetwork` to `WorldState` (or a new `SimulationState` container if preferred).

## Tests

| Test | What it verifies |
|------|------------------|
| `test_long_term_store_and_recall` | Store 10 entries, recall by keywords, assert relevance ordering |
| `test_long_term_importance_threshold` | Entries below 0.5 importance are not stored |
| `test_long_term_keyword_scoring` | More keyword overlap = higher recall rank |
| `test_short_term_eviction_returns_entry` | `add()` returns the evicted entry when at capacity |
| `test_gossip_create` | Creating gossip assigns id and sets source as knower |
| `test_gossip_propagate_60_percent` | Over 1000 trials, ~60% transmission rate |
| `test_gossip_distortion` | Over 1000 trials, ~20% of transmitted items are distorted |
| `test_gossip_no_duplicate_transmission` | Speaker can't transmit gossip listener already knows |
| `test_gossip_known_by` | Returns only gossip known by the queried NPC |
| `test_distortion_rules` | Each distortion rule produces a different string |
| `test_gossip_from_event` | DialogueOccurred event creates gossip item |

## Acceptance Criteria

- NPCs have long-term memory that persists across short-term evictions
- Memory recall returns contextually relevant entries
- Gossip spreads between NPCs during interactions with realistic probability
- Gossip content degrades over retelling
- Player can learn gossip through Tier 1 dialogue
- All tests passing
