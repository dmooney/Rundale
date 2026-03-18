# ADR-006: Natural Language Input

> Back to [ADR Index](README.md) | [Docs Index](../index.md)

## Status

Accepted (2026-03-18)

## Context

Parish is a text adventure where the player's primary interaction is typing text. Traditional text adventures use rigid parsers that expect specific verb-noun combinations ("GET LAMP", "GO NORTH"), leading to the infamous "guess the verb" problem that frustrates players.

Since Parish already has LLM inference available for NPC cognition (ADR-005), the same infrastructure can be used to parse player input. The question is whether to use a traditional parser, a hybrid approach, or full natural language understanding.

Additionally, the game needs system commands (save, load, quit, etc.) that must be reliably detected without ambiguity.

## Decision

Use **undecorated natural language input** as the primary interaction method, parsed by LLM inference into structured intent JSON.

**Player input flow:**

1. Player types free-form text (e.g., "Tell Mary I saw her husband at the crossroads")
2. Input is first checked against system commands (currently `/` prefixed)
3. If not a system command, input is sent to the LLM for intent parsing
4. LLM returns structured JSON:

```json
{
  "intent": "move|talk|look|interact|examine",
  "target": "location_id|npc_id|item_id",
  "dialogue": "what the player is saying (if talking)",
  "clarification_needed": false
}
```

5. If the LLM cannot resolve intent, the game asks for clarification in-character

**System commands** use a `/` prefix for reliable detection:

- `/pause`, `/resume`, `/quit`, `/save`, `/fork <name>`, `/load <name>`, `/branches`, `/log`, `/status`, `/help`, `/map`

**Future evolution**: The `/` prefix may be replaced with prefix-free fuzzy matching. The system would detect exact/fuzzy matches against the small fixed command set and show an inline confirmation prompt ("Quit the game? y/n"). If the player declines, the input passes through to the game world. False positives are harmless because of the confirmation step.

## Consequences

**Positive:**

- No "guess the verb" frustration: players type naturally and the system understands
- Flexible expression: "go to the pub", "head to O'Brien's", "walk down to the bar" all work
- Rich dialogue input: players can say anything to NPCs in their own words
- Consistent with the game's literary, immersive tone
- System commands are unambiguous with the `/` prefix

**Negative:**

- Every player input incurs an inference call, adding latency (~2-5 seconds per command)
- LLM may misparse ambiguous input, leading to unexpected game actions
- Inference cost per input reduces the available GPU budget for NPC cognition
- The `/` prefix for system commands breaks immersion slightly (addressed in future prefix-free design)
- Players accustomed to instant parser response may find the latency jarring

## Alternatives Considered

- **Traditional parser**: Deterministic and instant but rigid. Players must learn the exact verb set and syntax. The "guess the verb" problem is a well-known frustration in interactive fiction. Would feel archaic given the LLM-driven NPC system.
- **Keyword matching**: Extract keywords from input and map to actions via lookup table. Faster than LLM parsing but brittle. Cannot handle complex or nuanced input like "tell Mary I saw her husband at the crossroads."
- **Choice menus / multiple choice**: Present numbered options for the player to select. Reliable but fundamentally changes the genre from text adventure to choose-your-own-adventure. Limits player agency and expression.
- **Hybrid parser + LLM fallback**: Use a fast deterministic parser for common commands and fall back to LLM for complex input. Viable optimization for the future but adds complexity. The pure LLM approach is simpler to implement first.

## Related

- [docs/design/player-input.md](../design/player-input.md)
- [docs/design/inference-pipeline.md](../design/inference-pipeline.md)
- [ADR-005: Ollama Local Inference](005-ollama-local-inference.md)
- [ADR-008: Structured JSON LLM Output](008-structured-json-llm-output.md)
