# Cognitive Level-of-Detail System

> Parent: [Architecture Overview](overview.md) | [Docs Index](../index.md) | ADR: [002](../adr/002-cognitive-lod-tiers.md)

The simulation runs at four fidelity levels based on distance from the player. This is the core innovation of Parish: hundreds of NPCs maintain ongoing lives at varying levels of detail, creating a living world that continues whether or not the player is watching.

## Tier 1 — Immediate (GPU, 14B model)

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

- Lighter inference, shorter prompts, summary-level reasoning
- NPCs interact with each other at reduced depth
- The player may overhear or learn about these interactions
- **Capacity**: ~10-20 NPCs
- **Tick rate**: every few game-minutes

## Tier 3 — Distant (GPU, batch inference)

- Bulk tick: one LLM call covers many NPCs
- Prompt: "Here are 50 NPCs and their current states. Simulate N hours of activity. Return updated states."
- Broad strokes: relationships shift, resources change, major events occur
- **Tick rate**: every in-game day or two (~every few real-world minutes)

## Tier 4 — Far Away (CPU only, no LLM)

- Pure rules engine, deterministic or lightly randomized state transitions
- Births, deaths, trade, seasonal changes, national-level events
- Runs on 13900KS E-cores — low priority, high parallelism
- **Tick rate**: once per in-game season (~every 30-45 real-world minutes)
- Events from this tier filter down as news/gossip through NPC conversations

## Tier Transitions

When a player moves toward a distant NPC, that NPC's sparse state must be "inflated" into a rich context for real-time interaction. This is a prompt engineering problem:

> "You are [name]. Here's your personality. Here's what you've been up to lately: [summary from distant tick]. The player just arrived. Continue naturally."

An event bus must propagate state changes across tier boundaries to maintain coherence (e.g., if a nearby NPC decides to betray a distant one).

## Related

- [Inference Pipeline](inference-pipeline.md) — Queue architecture and model selection per tier
- [NPC System](npc-system.md) — Entity data model and context construction
- [ADR 002: Cognitive LOD Tiers](../adr/002-cognitive-lod-tiers.md)

## Source Modules

- [`src/npc/`](../../src/npc/) — NPC behavior, cognition tiers
- [`src/inference/`](../../src/inference/) — Ollama HTTP client, inference queue
- [`src/world/`](../../src/world/) — World state, location graph
