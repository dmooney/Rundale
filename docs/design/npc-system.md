# NPC System

> Parent: [Architecture Overview](overview.md) | [Docs Index](../index.md)

## Entity Data Model

Each NPC has:

- **Identity**: Name, age, physical description, occupation
- **Personality**: Traits, values, temperament (used as LLM system prompt)
- **Location**: Current node, home node, workplace node
- **Schedule**: Daily routine patterns (varies by day of week, season, weather)
- **Relationships**: Weighted edges to other NPCs (family, friend, rival, enemy, romantic, etc.)
- **Memory**:
  - Short-term: Last few interactions, current goals, immediate observations
  - Long-term: Key events, major relationship changes, grudges, secrets
  - Consider embedding-based retrieval for relevant long-term memories
- **Physical State**: Health, energy, hunger (if applicable)
- **Knowledge**: What they know about the world — public events, gossip, secrets

## NPC Context Construction

For each LLM inference call, build a context from these five layers:

1. **System prompt**: personality, backstory, current emotional state
2. **Public knowledge**: weather, time, season, major recent events
3. **Personal knowledge**: their relationships, recent experiences, secrets
4. **Immediate situation**: where they are, who's present, what just happened
5. **Conversation history** (if in dialogue)

## Gossip & Information Propagation

NPCs share information through conversation. A public event gets injected into the shared knowledge base. Private information (gossip, secrets) spreads through NPC-to-NPC interactions, potentially getting distorted. The player can learn about offscreen events through NPC dialogue organically.

## Structured Output Schema

All LLM responses for NPC behavior should be structured JSON:

```json
{
  "action": "speak|move|trade|work|rest|observe",
  "target": "player|npc_id|location|item",
  "dialogue": "What they say (if speaking)",
  "mood": "current emotional state",
  "internal_thought": "what they're actually thinking (hidden from player)",
  "knowledge_gained": ["any new information learned"],
  "relationship_changes": [{"npc_id": "...", "delta": 0.0}]
}
```

## Related

- [Cognitive LOD](cognitive-lod.md) — Tier system determines inference fidelity per NPC
- [Inference Pipeline](inference-pipeline.md) — How NPC context is sent to Ollama
- [Weather System](weather-system.md) — Weather affects NPC schedules and behavior
- [World & Geography](world-geography.md) — NPCs are bound to location nodes
- [ADR 008: Structured JSON LLM Output](../adr/008-structured-json-llm-output.md)

## Source Modules

- [`src/npc/`](../../src/npc/) — NPC data model, behavior, cognition tiers
