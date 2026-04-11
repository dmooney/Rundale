# ADR-002: Cognitive Level-of-Detail Tiers

> Back to [ADR Index](README.md) | [Docs Index](../index.md)

## Status

Accepted (2026-03-18)

## Context

The core innovation of Rundale is a living world where hundreds of NPCs have ongoing lives, relationships, and conversations, whether or not the player is watching. However, full LLM inference for every NPC on every tick is impossible given the hardware constraints (RX 9070 16GB GPU, ~30-50 tokens/sec on a 14B model, ~100-150 tokens per NPC response).

At best, the system can handle ~3-5 full-fidelity NPC "thoughts" per second. With hundreds of NPCs in the world, a uniform simulation approach would either exhaust GPU resources instantly or require unacceptable simplification of every NPC.

The system needs a way to allocate inference resources proportionally to proximity and relevance to the player, while still maintaining the illusion that every NPC in the world is living their life.

## Decision

Implement a **4-tier cognitive level-of-detail (LOD) system** based on distance from the player:

**Tier 1 -- Immediate (GPU, 14B model)**
- Full LLM inference per NPC interaction
- Rich dialogue, nuanced decisions, emotional responses, awareness of player actions
- Real-time, per-interaction inference
- Capacity: ~3-5 NPCs simultaneously
- Structured JSON output with action, dialogue, mood, internal thought, knowledge, relationship changes

**Tier 2 -- Nearby (GPU, 8B or 3B model)**
- Lighter inference with shorter prompts and summary-level reasoning
- NPCs interact with each other at reduced depth
- The player may overhear or learn about these interactions
- Capacity: ~10-20 NPCs
- Tick rate: every few game-minutes

**Tier 3 -- Distant (GPU, batch inference)**
- Bulk tick: one LLM call covers many NPCs
- Prompt pattern: "Here are 50 NPCs and their current states. Simulate N hours of activity. Return updated states."
- Broad strokes: relationships shift, resources change, major events occur
- Tick rate: every in-game day or two (~every few real-world minutes)

**Tier 4 -- Far Away (CPU only, no LLM)**
- Pure rules engine with deterministic or lightly randomized state transitions
- Births, deaths, trade, seasonal changes, national-level events
- Runs on CPU E-cores at low priority with high parallelism
- Tick rate: once per in-game season (~every 30-45 real-world minutes)
- Events from this tier filter down as news/gossip through NPC conversations

When a player moves toward a distant NPC, that NPC's sparse state is "inflated" into rich context for real-time interaction via prompt engineering: "You are [name]. Here's your personality. Here's what you've been up to lately: [summary from distant tick]. The player just arrived. Continue naturally."

An event bus propagates state changes across tier boundaries to maintain coherence.

## Consequences

**Positive:**

- Creates the illusion of a living world with hundreds of active NPCs
- GPU resources are focused where they matter most: direct player interaction
- Scalable: adding more NPCs only costs Tier 3/4 simulation budget
- Distant events create organic gossip and news that flow to the player through NPC dialogue
- Each tier can be developed and tuned independently

**Negative:**

- Tier transitions (state inflation/deflation) are complex prompt engineering problems
- Potential coherence issues across tier boundaries if event propagation fails
- Tier 2/3 NPCs may behave inconsistently when promoted to Tier 1
- Tuning tick rates and model allocation requires empirical testing
- Four separate simulation subsystems to build and maintain

## Alternatives Considered

- **Uniform simulation**: Run every NPC at the same fidelity. Impossible at scale with local hardware. Even with a 3B model, hundreds of per-tick inferences would saturate the GPU.
- **Only simulate visible NPCs**: Simple but creates a "dead world" feeling. NPCs would have no history or agency when the player arrives. Conversations would lack depth because nothing happened while the player was away.
- **Pre-scripted behaviors**: Traditional game AI with state machines and scripted routines. Loses the emergent behavior that is the core design goal. NPCs would feel robotic and predictable.

## Related

- [docs/design/cognitive-lod.md](../design/cognitive-lod.md)
- [docs/design/inference-pipeline.md](../design/inference-pipeline.md)
- [ADR-005: Ollama Local Inference](005-ollama-local-inference.md)
- [ADR-008: Structured JSON LLM Output](008-structured-json-llm-output.md)
