# Inference Pipeline

> Parent: [Architecture Overview](overview.md) | [Docs Index](../index.md) | ADRs: [005](../adr/005-ollama-local-inference.md), [010](../adr/010-prompt-injection-defenses.md)

## Pipeline Architecture

```
                  ┌─ Interactive lane (cap 16) ─┐
Simulation Tiers ─┼─ Background  lane (cap 32) ─┼─► Single-flight Worker ─► OpenAI-compatible API ─► Response Router ─► World State
                  └─ Batch       lane (cap 64) ─┘
```

The inference queue is **one** `InferenceQueue` struct (`crates/parish-inference/src/lib.rs:124`) wrapping **three** Tokio mpsc channels — one per priority lane. A single worker task drains them in strict priority order.

### Priority Lanes

| Lane        | Capacity | Used for |
|-------------|----------|----------|
| Interactive | 16       | Tier 1 player-facing dialogue (streaming) |
| Background  | 32       | Tier 2 nearby NPC simulation (JSON) |
| Batch       | 64       | Tier 3 distant NPC batch simulation (JSON) |

Capacities are set at queue construction in each frontend — see `crates/parish-server/src/routes.rs:205-207`, `crates/parish-tauri/src/commands.rs:305-307`, and `crates/parish-cli/src/headless.rs:58-60`. They are sized so bursts of background or batch work cannot block an incoming interactive request from reaching the worker.

### Single-Flight Worker

`spawn_inference_worker` (`crates/parish-inference/src/lib.rs:453`) runs one LLM call at a time using `tokio::select!` with biased ordering:

```rust
tokio::select! {
    biased;
    Some(req) = interactive_rx.recv() => req,
    Some(req) = background_rx.recv() => req,
    Some(req) = batch_rx.recv() => req,
    else => break,
}
```

`biased;` makes the select check lanes top-down every iteration, so an Interactive request always beats any pending Background or Batch request. There is **no preemption mid-request** — if a Batch call is in-flight when an Interactive request arrives, the Interactive request waits for the in-flight call to return. Priority applies at lane selection, not inside the LLM call.

## Inference Use Cases

Parish makes LLM calls from five inbound paths. Three go through the priority queue; two bypass it by resolving a per-category client directly via `GameConfig::resolve_category_client()` (`crates/parish-core/src/ipc/config.rs:90`).

| Use case                   | Category   | Path                       | Streaming      | Output              | Call site |
|----------------------------|------------|----------------------------|----------------|---------------------|-----------|
| Player dialogue (Tier 1)   | Dialogue   | Interactive lane           | Yes            | Text + JSON tail    | `crates/parish-tauri/src/commands.rs:825` (and server / CLI equivalents) |
| Nearby NPC sim (Tier 2)    | Simulation | Background lane            | No             | JSON                | `crates/parish-npc/src/ticks.rs:533` |
| Distant NPC batch (Tier 3) | Simulation | Batch lane                 | No             | JSON                | `crates/parish-npc/src/ticks.rs:853` |
| NPC arrival reactions      | Reaction   | Direct call (bypass queue) | Optional       | Plain text, ≤100 tok | `crates/parish-npc/src/reactions.rs:876` |
| Player intent parsing      | Intent     | Direct call (bypass queue) | No             | JSON                | `crates/parish-tauri/src/commands.rs:495-503` |

Queue-based calls compete for the single in-flight worker slot. Direct-category calls run concurrently on their own per-category `OpenAiClient` instances, limited only by each provider's HTTP connection pool. Effective parallelism is therefore `1 (worker) + N (direct-category clients, one per Intent/Reaction call in flight)`.

Reaction timeouts are caller-supplied (the `reactions.rs` helper takes `timeout_secs: u64`), not hardcoded on the queue side.

## Throughput Estimates

- 9B-class local model (Ollama, q4) on RX 9070: **~40-60 tokens/sec**
- At ~100-150 tokens per NPC response: **~3-6 NPC "thoughts" per second**
- Cloud providers (Claude Sonnet 4.6, Gemini 2.5 Flash) are typically faster per-token than local but add ~300-1000 ms network round-trip; budget ~1-2 s per Tier 1 response end-to-end.
- Numbers vary with model, quantization, and prompt length — measure on your own hardware before tuning tick intervals.

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

## NPC Context Construction (Tier 1)

The enhanced context sent to the LLM for Tier 1 NPC dialogue is built from multiple layers:

1. **System prompt** (`build_enhanced_system_prompt`): Identity, historical context, cultural guidelines, personality, intelligence guidance, mood, relationships (by name), knowledge, improv craft (optional)
2. **Context prompt** (`build_enhanced_context`): Location + description, time/season/weather, who else is present (with relationship context), recent conversation history at this location (last 3 exchanges), scene continuity cue (if already in conversation), short-term memories, player reactions, long-term memory recall, gossip context
3. **Player input**: The raw text the player typed

### Post-Response Processing

After the LLM responds, all modes execute the same pipeline:

1. `apply_tier1_response` — updates NPC mood, records speaker's own memory
2. `conversation_log.add()` — records the exchange in the per-location conversation log
3. `record_witness_memories()` — creates "Overheard" memory entries for all other NPCs at the location

## Multi-Provider Support

The pipeline supports any OpenAI-compatible endpoint (Ollama, LM Studio, OpenRouter, Google Gemini, Groq, xAI, Mistral, DeepSeek, Together, vLLM, or any custom endpoint) via `OpenAiClient`. Per-category provider routing lets different inbound paths use different models. The engine defines **four** categories, resolved by `GameConfig::resolve_category_client()`:

| Category   | Purpose                                              | Default |
|------------|------------------------------------------------------|---------|
| Dialogue   | Player-facing NPC conversation (Tier 1)              | Cloud if configured, else base provider |
| Simulation | Background NPC sim (Tier 2 + Tier 3 batch)           | Base provider (usually local) |
| Intent     | Player input classification (direct, low-latency)    | Base provider (usually local) |
| Reaction   | NPC arrival greetings (direct, short timeout)        | Base provider (usually local) |

Configuration is runtime-mutable via `/provider`, `/model`, `/key`, and `/cloud` commands. Changing provider settings respawns the inference worker with a new client and swaps per-category clients atomically.

### Recommended Models (April 2026)

> This section is **refreshable** — specific picks will drift as the open-model landscape evolves. Last refresh: April 2026. See ADR-005 for the architectural decision; this section owns the specific picks.

Hardware baseline: RX 9070 16 GB + i9-13900KS (matches ADR-005).

| Category                    | Local pick                 | Cloud pick                          | Why |
|-----------------------------|----------------------------|-------------------------------------|-----|
| Dialogue                    | Gemma 4 9B or Qwen 3.5 9B  | Claude Sonnet 4.6                   | Quality-critical; 9B fits in 16 GB VRAM with headroom |
| Simulation (Tier 2 nearby)  | Qwen 3.5 9B                | Gemini 2.5 Flash                    | Structured JSON throughput matters more than prose quality |
| Simulation (Tier 3 batch)   | Qwen 3.5 9B                | **Gemini 2.5 Flash-Lite**           | $0.10 / $0.40 per 1M tokens makes cloud Tier 3 effectively free at game scale; stack with batch API + prompt caching |
| Intent                      | Ministral 3 3B             | — (always local)                    | Low-latency JSON / function-calling; 3B is enough and keeps the player's input path private |
| Reaction                    | Ministral 3 3B             | Gemini 2.5 Flash-Lite               | Short, fast responses; shares the 3B model with Intent |

Notes on the picks:

- **Gemma 4** (Apache 2.0, April 2, 2026) tends to be stronger at naturalistic prose. **Qwen 3.5 9B** (Feb 2026) tends to be stronger at structured output. Qwen 3.5 does not ship a 14B size — 9B is the new Tier 1 target, superseding the Qwen3 14B reference from ADR-005.
- **Ministral 3 3B** ships with first-class JSON / function-calling, which is exactly what Intent and Reaction need.
- **Claude Sonnet 4.6** remains the quality leader for in-character dialogue if you have a cloud budget.
- Benchmarks don't measure 1820 Irish peasant dialogue. Build a small fixture and use the `/prove` harness before committing any model to production.

### Starter Configurations

**Cloud-light** — cloud quality where it matters, cheap batch, local intent/reaction:

```toml
[provider]
name = "ollama"
base_url = "http://localhost:11434"
model = "ministral3:3b"

[provider.dialogue]
name = "openrouter"
model = "anthropic/claude-sonnet-4-6"
api_key = "$OPENROUTER_API_KEY"

[provider.simulation]
name = "google"
model = "gemini-2.5-flash-lite"
api_key = "$GOOGLE_API_KEY"
```

**Fully-local** — zero cloud dependency; run two Ollama instances on different ports so the larger model stays loaded for Dialogue/Simulation while the 3B handles Intent/Reaction. The engine's built-in auto-selector picks a gemma4 tier based on VRAM / unified memory (see `select_model_for_vram` in `crates/parish-inference/src/setup.rs`); override here if you want something different:

```toml
[provider]
name = "ollama"
base_url = "http://localhost:11434"
model = "gemma4:e4b"   # or gemma4:26b (MoE) / gemma4:31b (dense) if you have the memory

[provider.intent]
name = "ollama"
base_url = "http://localhost:11435"
model = "ministral3:3b"

[provider.reaction]
name = "ollama"
base_url = "http://localhost:11435"
model = "ministral3:3b"
```

**Quality-maximalist** — full cloud, everything routed via one provider for simplicity:

```toml
[provider]
name = "openrouter"
model = "google/gemini-2.5-flash-lite"
api_key = "$OPENROUTER_API_KEY"

[provider.dialogue]
name = "openrouter"
model = "anthropic/claude-sonnet-4-6"
api_key = "$OPENROUTER_API_KEY"

[provider.simulation]
name = "openrouter"
model = "google/gemini-3.1-pro"
api_key = "$OPENROUTER_API_KEY"
```

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
- **Scope**: Captures all requests that flow through the worker — Tier 1 dialogue (Interactive lane) plus Tier 2 and Tier 3 simulation (Background and Batch lanes, via `submit_json`). Direct-category calls (Intent, Reaction) run outside the worker and are not captured here.
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
