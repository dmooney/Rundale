# Inference Pipeline

> Parent: [Architecture Overview](overview.md) | [Docs Index](../index.md) | ADRs: [005](../adr/005-ollama-local-inference.md), [010](../adr/010-prompt-injection-defenses.md)

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

## Multi-Provider Support

The pipeline supports any OpenAI-compatible endpoint (Ollama, LM Studio, OpenRouter, etc.) via `OpenAiClient`. Per-category provider routing allows different models for different tasks:

| Category | Purpose | Default |
|----------|---------|---------|
| Dialogue | Player-facing NPC conversation (Tier 1) | Cloud if configured, else local |
| Simulation | Background NPC activity (Tier 2) | Always local |
| Intent | Player input classification | Always local |

Configuration is runtime-mutable via `/provider`, `/model`, `/key`, and `/cloud` commands. Changing provider settings respawns the inference worker with a new client.

## Inference Call Logging

Every request processed by the inference worker is logged in a shared ring buffer (`InferenceLog`) for real-time visibility in the debug panel.

### `InferenceLogEntry`

```rust
pub struct InferenceLogEntry {
    pub request_id: u64,            // Unique request ID
    pub timestamp: String,           // Wall-clock time (HH:MM:SS)
    pub model: String,               // Model name used
    pub streaming: bool,             // Whether SSE streaming was used
    pub duration_ms: u64,            // End-to-end latency
    pub prompt_len: usize,           // Prompt length in characters
    pub response_len: usize,         // Response length in characters
    pub error: Option<String>,       // Error message if failed
    pub system_prompt: Option<String>, // System prompt (if any)
    pub prompt_text: String,         // Full user prompt
    pub response_text: String,       // Full response text
    pub max_tokens: Option<u32>,     // Token limit (if set)
}
```

### Architecture

```
InferenceRequest → spawn_inference_worker() → generate()/generate_stream()
                         │                              │
                         │  records Instant::now()       │  measures elapsed
                         │                              │
                         └──── InferenceLogEntry ───────┘
                                      │
                              InferenceLog (Arc<Mutex<VecDeque>>)
                                      │
                              DebugSnapshot.inference.call_log
                                      │
                              Tauri IPC → Svelte DebugPanel
```

- **Capacity**: 50 entries (ring buffer, oldest evicted first)
- **Scope**: Captures all queued requests (NPC dialogue). Direct `generate_json()` calls (Tier 2 simulation, intent parsing) are not yet logged.
- **Shared state**: The `InferenceLog` (`Arc<Mutex<VecDeque<InferenceLogEntry>>>`) is passed to the worker at spawn time and stored on `AppState` for snapshot reads.
- **Timing**: `std::time::Instant` measures end-to-end latency including network round-trip, model inference, and streaming delivery.

### Debug Panel Display

The Inference tab in the debug panel shows:

1. **Config section** (top): Provider, model, URL, queue status, cloud info, improv flag
2. **Call Log section** (below): Summary stats (avg latency, error count) followed by a scrollable list of entries (newest first) with color-coded OK/ERROR/STREAM badges

## Related

- [NPC System](npc-system.md) — NPC context construction feeds the inference queue
- [Cognitive LOD](cognitive-lod.md) — Tier determines model selection and batch strategy
- [Player Input](player-input.md) — Natural language input parsed via this pipeline
- [Debug UI](debug-ui.md) — Debug panel that displays inference call log
- [ADR 005: Ollama Local Inference](../adr/005-ollama-local-inference.md)
- [ADR 008: Structured JSON LLM Output](../adr/008-structured-json-llm-output.md)

## Source Modules

- [`crates/parish-core/src/inference/`](../../crates/parish-core/src/inference/) — OpenAI-compatible HTTP client, inference queue, worker, log
- [`crates/parish-core/src/debug_snapshot.rs`](../../crates/parish-core/src/debug_snapshot.rs) — `InferenceLogEntry`, `InferenceDebug` structs
- [`src/inference/`](../../src/inference/) — Root crate inference (headless mode)
- [`src/input/`](../../src/input/) — Player input parsing
- [`src/npc/`](../../src/npc/) — NPC context construction
