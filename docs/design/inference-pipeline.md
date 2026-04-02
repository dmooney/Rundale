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

## Web Server Inference Path

The `parish-server` crate provides a browser-accessible game mode via axum (HTTP + WebSocket). Its inference pipeline mirrors the Tauri path but has distinct characteristics worth noting.

### EventBus

Server-push events (world snapshots, theme updates, NPC streaming tokens, text log entries) are broadcast to WebSocket clients via `EventBus` (`crates/parish-server/src/state.rs`):

- `send()` — returns the receiver count; logs `tracing::warn!` if the channel has no active subscribers (capacity 256, drop-on-overflow for slow receivers).
- `emit()` — serialises the payload to `serde_json::Value` first; logs `tracing::warn!` if serialisation fails so silent event loss is observable in structured logs.

### Provider Rebuild

When the player issues `/provider` or `/key` commands, `rebuild_inference()` in `routes.rs` respawns the worker with a new `OpenAiClient`. The lock ordering is:

1. Acquire and release `config` lock in a scoped block.
2. Acquire and release `client` lock.
3. Spawn inference worker (no lock held).
4. Acquire `inference_queue` lock and replace the queue.

Config is released before any other lock is acquired to minimise the race window between concurrent rebuild calls.

### Inference Availability Check

`handle_npc_conversation()` checks the inference queue presence together with NPC presence in a single locked block. The two failure cases are distinguished:

| Condition | Response to player |
|-----------|-------------------|
| No NPC at current location, queue absent or present | Random idle-world flavour message |
| NPC present, but `inference_queue` is `None` | Clear message: "There's someone here, but the LLM is not configured — set a provider with /provider." |

This prevents the confusing case where the player tries to speak to a character and receives a "wind stirs" message with no indication that the LLM is unconfigured.

### Background Tasks

Two fire-and-forget tasks are spawned in `spawn_background_ticks()`:

| Task | Interval | Purpose |
|------|----------|---------|
| World tick | 5 s | Broadcasts `world-update` snapshot; ticks NPC schedules |
| Theme tick | 500 ms | Broadcasts `theme-update` palette |

Both log `tracing::debug!` at startup. Serialisation errors inside either loop surface via `EventBus::emit()`'s warn logging. The Tokio runtime logs task panics automatically; no additional panic wrappers are used.

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
