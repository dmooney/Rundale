# NPC System

> Parent: [Architecture Overview](overview.md) | [Docs Index](../index.md) | ADR: [008](../adr/008-structured-json-llm-output.md)

## Entity Data Model

Each NPC has:

- **Identity**: Name, age, physical description, occupation
- **Personality**: Traits, values, temperament (used as LLM system prompt)
- **Intelligence**: Multidimensional profile (6 axes, 1-5 scale) — injected as direct behavioral guidance, not coded tags — see [ADR 018](../adr/018-npc-intelligence-dimensions.md)
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

For each LLM inference call, build a context from these layers:

1. **System prompt**: personality, intelligence guidance (behavioral directives only), current emotional state. NPC dialogue is pure speech — no parenthetical stage directions. Physical actions are tracked in JSON metadata only.
2. **Public knowledge**: weather, time, season, major recent events
3. **Personal knowledge**: their relationships (by name), recent experiences, secrets
4. **Immediate situation**: where they are, who's present (with relationship context), what just happened
5. **Conversation history**: recent exchanges at this location (last 3), with scene continuity cues
6. **Witness awareness**: overheard conversations from bystander memory

## Conversation Awareness

NPCs are aware of conversations happening around them, not just conversations directed at them.

### Witness Memory System

When the player talks to NPC A at a location where NPCs B and C are also present:
- B and C each receive a short-term memory entry: `"Overheard: a traveller said '...' and {A} replied '...'"`
- These memories appear in B's and C's context when the player talks to them next
- This creates natural conversational flow: "I heard what you said to Padraig..."

### Conversation Log

A per-location ring buffer (`ConversationLog` on `WorldState`) tracks the last 30 exchanges globally. Recent exchanges at the current location are injected into the context prompt under "What's been said here", giving NPCs awareness of what's been discussed.

### Scene Continuity

If the player has recently spoken to the same NPC, a cue is injected: "You are already in conversation with this traveller. Do not re-introduce yourself." This prevents NPCs from re-greeting on every utterance.

### Prompt Quality

- Relationships reference NPCs by name ("Niamh Darcy: Family, very close") not by ID
- "Also present" includes relationship context ("Niamh Darcy, the Publican's Daughter — Family to you")
- Knowledge framed as "WHAT'S ON YOUR MIND" for natural grounding

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

- [`crates/parish-core/src/npc/`](../../crates/parish-core/src/npc/) — NPC data model, behavior, cognition tiers
- [`crates/parish-core/src/npc/conversation.rs`](../../crates/parish-core/src/npc/conversation.rs) — ConversationLog, ConversationExchange
- [`crates/parish-core/src/npc/ticks.rs`](../../crates/parish-core/src/npc/ticks.rs) — Prompt builders, witness memories, response processing
- [`crates/parish-core/src/npc/memory.rs`](../../crates/parish-core/src/npc/memory.rs) — Short-term and long-term memory
