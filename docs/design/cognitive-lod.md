# Cognitive Level-of-Detail System

> Parent: [Architecture Overview](overview.md) | [Docs Index](../index.md) | ADR: [002](../adr/002-cognitive-lod-tiers.md)

The simulation runs at four fidelity levels based on distance from the player. This is the core innovation of Parish: hundreds of NPCs maintain ongoing lives at varying levels of detail, creating a living world that continues whether or not the player is watching.

## Tier 1 — Immediate (GPU, 14B model)

- **Distance**: 0 (same location as player)
- Full LLM inference per NPC interaction
- Rich dialogue, nuanced decisions, emotional responses, awareness of player actions
- Real-time, per-interaction inference
- **Capacity**: ~3-5 NPCs simultaneously
- Structured JSON output:

```json
{
  "action": "...",
  "target": "...",
  "dialogue": "...",
  "mood": "...",
  "internal_thought": "..."
}
```

## Tier 2 — Nearby (GPU, 8B or 3B model)

- **Distance**: 1-2 edges from player
- Lighter inference, shorter prompts, summary-level reasoning
- NPCs interact with each other at reduced depth
- The player may overhear or learn about these interactions
- **Capacity**: ~10-20 NPCs
- **Tick rate**: every 5 game-minutes

## Tier 3 — Distant (GPU, batch inference)

- **Distance**: 3-4 edges from player
- **Implementation** (`npc/tier3.rs`): Processes 8 NPCs per LLM batch call once per game-day
- Prompt sends current states of the batch and requests updated states, relationship changes, and summary events
- Broad strokes: relationships shift, resources change, major events occur
- **Tick rate**: every in-game day (~every few real-world minutes)

## Tier 4 — Far Away (CPU only, no LLM)

- **Distance**: 5+ edges from player
- **Implementation** (`npc/tier4.rs`): Pure rules engine with seasonal tick, no LLM calls
- Deterministic or lightly randomized state transitions
- Births, deaths, trade, seasonal changes, national-level events
- Runs on 13900KS E-cores — low priority, high parallelism
- **Tick rate**: once per in-game season (~every 30-45 real-world minutes)
- Events from this tier filter down as news/gossip through NPC conversations

## Tier Transitions

**Implementation** (`npc/ticks.rs`): When a player moves toward a distant NPC, that NPC's tier changes and its state is inflated or deflated accordingly.

- **Inflate** (e.g., Tier 4 -> Tier 1): Reconstructs rich context from sparse state. Long-term memory entries, recent gossip, and seasonal summary are composed into a system prompt preamble so the NPC can continue naturally.
- **Deflate** (e.g., Tier 1 -> Tier 3): Compresses detailed short-term memory and recent interactions into summary entries for long-term memory. Active conversation state is discarded.

The **event bus** (`world/events.rs`) propagates state changes across tier boundaries using `tokio::sync::broadcast` (capacity 256) to maintain coherence (e.g., weather changes affect all tiers, gossip spreads between co-located NPCs regardless of tier). See [ADR 018](../adr/018-tokio-broadcast-event-bus.md).

## Related

- [Inference Pipeline](inference-pipeline.md) — Queue architecture and model selection per tier
- [NPC System](npc-system.md) — Entity data model and context construction
- [ADR 002: Cognitive LOD Tiers](../adr/002-cognitive-lod-tiers.md)

## Source Modules

- [`src/npc/`](../../src/npc/) — NPC behavior, cognition tiers
- [`src/inference/`](../../src/inference/) — Ollama HTTP client, inference queue
- [`src/world/`](../../src/world/) — World state, location graph
