# ADR-008: Structured JSON LLM Output

> Back to [ADR Index](README.md) | [Docs Index](../index.md)

## Status

Accepted (2026-03-18)

## Context

Parish uses LLM inference for two primary purposes: NPC behavior/cognition and player input parsing. In both cases, the LLM output must be machine-parseable to update world state programmatically. The game cannot rely on free-text output that requires human interpretation.

Requirements:

- NPC responses must decompose into discrete actions, dialogue, emotional state, and world state changes
- Player input must be mapped to specific game intents with identified targets
- Parsing must be reliable enough for automated processing with minimal human intervention
- The output format must work well with local models (Qwen3 family) that may have limited instruction-following compared to frontier models
- Hidden information (NPC internal thoughts) must be captured but not shown to the player

## Decision

All LLM responses use **structured JSON schemas** enforced via prompt engineering.

**NPC behavior response schema:**

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

**Player input parsing schema:**

```json
{
  "intent": "move|talk|look|interact|examine",
  "target": "location_id|npc_id|item_id",
  "dialogue": "what the player is saying (if talking)",
  "clarification_needed": false
}
```

**Implementation details:**

- Schemas are included in system prompts for each inference call
- Use `#[serde(default)]` on all optional fields in Rust response structs to handle missing fields gracefully
- Implement fallback parsing: if JSON parsing fails, attempt to extract partial information or request clarification
- The `internal_thought` field enables rich NPC reasoning without exposing it to the player, creating potential for dramatic irony

## Consequences

**Positive:**

- Reliable, automated parsing of LLM output into game state updates
- Consistent behavior schema across all NPCs and tiers
- The `internal_thought` field creates depth: NPCs can think one thing and say another
- `relationship_changes` enable automatic social graph updates
- `knowledge_gained` feeds the gossip and information propagation system
- Structured output is easier to log, debug, and replay than free text
- JSON is well-represented in LLM training data, leading to better adherence

**Negative:**

- Constrains model creativity: the model must fit its response into predefined fields
- JSON formatting tokens consume part of the response budget (~20-30 tokens of overhead per response)
- Parsing failures require fallback handling, adding complexity
- Schema evolution requires updating prompts across all tiers
- Local models may occasionally produce malformed JSON, requiring robust error handling
- The fixed action set (speak, move, trade, work, rest, observe) may not cover all emergent behaviors

## Alternatives Considered

- **Free text with regex extraction**: Parse natural language output using regular expressions or pattern matching. Extremely fragile. LLMs express the same information in many different ways, making reliable extraction nearly impossible without extensive regex engineering.
- **Function calling / tool use**: Some LLM APIs support structured function calling. However, local models accessed via Ollama have inconsistent support for this feature. Relying on it would limit model choice and add a brittle dependency.
- **XML output**: More verbose than JSON and less common in LLM training data, leading to worse adherence. XML parsing in Rust is also less ergonomic than JSON via serde.
- **Custom DSL**: Define a domain-specific output language. Requires training or heavy few-shot prompting for the model to learn. JSON is already a well-known "DSL" that models handle well out of the box.

## Related

- [docs/design/npc-system.md](../design/npc-system.md)
- [docs/design/inference-pipeline.md](../design/inference-pipeline.md)
- [ADR-002: Cognitive Level-of-Detail Tiers](002-cognitive-lod-tiers.md)
- [ADR-005: Ollama Local Inference](005-ollama-local-inference.md)
- [ADR-006: Natural Language Input](006-natural-language-input.md)
