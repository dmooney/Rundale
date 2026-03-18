# Inference Pipeline

> Parent: [Architecture Overview](overview.md) | [Docs Index](../index.md)

## Pipeline Architecture

```
Simulation Threads → Inference Queue (Tokio mpsc channel) → Inference Worker → Ollama REST API → Response Router → World State Update
```

- Inference queue accepts requests from any simulation tier
- A dedicated async task pulls requests, sends to Ollama, routes responses back
- Batch requests where possible (multiple Tier 2/3 NPCs in one call)

## Tiered Model Selection

| Tier | Use Case             | Model          |
|------|----------------------|----------------|
| 1    | Direct interaction   | Qwen3 14B      |
| 2    | Nearby activity      | Qwen3 8B or 3B |
| 3    | Distant batch        | Qwen3 8B or 3B with bulk prompts |

## Throughput Estimates

- Expected throughput with Qwen3 14B on RX 9070: **~30-50 tokens/sec**
- At ~100-150 tokens per NPC response: **~3-5 NPC "thoughts" per second**

## Player Input Parsing

Player natural language input is also sent to Ollama for intent parsing. The LLM maps free text to game actions:

```json
{
  "intent": "move|talk|look|interact|examine",
  "target": "location_id|npc_id|item_id",
  "dialogue": "what the player is saying (if talking)",
  "clarification_needed": false
}
```

If the LLM can't resolve intent, the game asks for clarification in-character.

## Related

- [NPC System](npc-system.md) — NPC context construction feeds the inference queue
- [Cognitive LOD](cognitive-lod.md) — Tier determines model selection and batch strategy
- [Player Input](player-input.md) — Natural language input parsed via this pipeline
- [ADR 005: Ollama Local Inference](../adr/005-ollama-local-inference.md)
- [ADR 008: Structured JSON LLM Output](../adr/008-structured-json-llm-output.md)

## Source Modules

- [`src/inference/`](../../src/inference/) — Ollama HTTP client, inference queue
- [`src/input/`](../../src/input/) — Player input parsing
- [`src/npc/`](../../src/npc/) — NPC context construction
