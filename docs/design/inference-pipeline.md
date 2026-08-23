# Inference Pipeline

> Status: Implemented · Updated: 2026-05-25 · [Docs Index](../index.md)

> Parent: [Architecture Overview](overview.md) | [Docs Index](../index.md) | ADRs: [005](../adr/005-ollama-local-inference.md), [008](../adr/008-structured-json-llm-output.md), [010](../adr/010-prompt-injection-defenses.md)
>
> Measurement record: local archive `docs/proofs/local-perf/evidence.md` — raw benchmark data, methodology, and reproductions for the macOS / Apple Silicon path.

## Per-Category Latency Budgets

The engine recognizes four inference categories. Each has a different latency expectation tied to its role in the player turn:

| Category   | ttft budget    | total budget            | Rationale                                                                                                |
| ---------- | -------------- | ----------------------- | -------------------------------------------------------------------------------------------------------- |
| Intent     | < 200 ms       | < 500 ms                | Player typed a command — every ms compounds onto every later turn                                        |
| Reaction   | < 400 ms       | < 800 ms                | NPC greeting on arrival; subsecond keeps the scene fluid                                                 |
| Simulation | < 800 ms       | < 1500 ms               | Background world tick; runs concurrently with player turn — must finish before next player input arrives |
| Dialogue   | < 1000 ms ttft | streaming, no total cap | First token must land quickly; rest streams under the player's reading speed                             |

These budgets are not enforced in code today — they are the success criteria for the `/inf-bench` harness (`crates/parish-inference/examples/inf_bench.rs`) and the gate against which provider/model choices are validated.

## Pipeline Architecture

```text
                  ┌─ Interactive lane (cap 16) ─┐
Simulation Tiers ─┼─ Background  lane (cap 32) ─┼─► Single-flight Worker ─► OpenAI-compatible API ─► Response Router ─► World State
                  └─ Batch       lane (cap 64) ─┘
```

The inference queue is **one** `InferenceQueue` struct (`crates/parish-inference/src/lib.rs:124`) wrapping **three** Tokio mpsc channels — one per priority lane. A single worker task drains them in strict priority order.

### Priority Lanes

| Lane        | Capacity | Used for                                   |
| ----------- | -------- | ------------------------------------------ |
| Interactive | 16       | Tier 1 player-facing dialogue (streaming)  |
| Background  | 32       | Tier 2 nearby NPC simulation (JSON)        |
| Batch       | 64       | Tier 3 distant NPC batch simulation (JSON) |

Capacities are set at queue construction in each frontend — see `crates/parish-server/src/routes.rs:205-207`, `crates/parish-tauri/src/commands.rs:305-307`, and `crates/parish-engine/src/headless.rs:58-60`. They are sized so bursts of background or batch work cannot block an incoming interactive request from reaching the worker.

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

| Use case                   | Category   | Path                       | Streaming | Output               | Call site                                                                |
| -------------------------- | ---------- | -------------------------- | --------- | -------------------- | ------------------------------------------------------------------------ |
| Player dialogue (Tier 1)   | Dialogue   | Interactive lane           | Yes       | Text + JSON tail     | `crates/parish-tauri/src/commands.rs:825` (and server / CLI equivalents) |
| Nearby NPC sim (Tier 2)    | Simulation | Background lane            | No        | JSON                 | `crates/parish-npc/src/ticks.rs:533`                                     |
| Distant NPC batch (Tier 3) | Simulation | Batch lane                 | No        | JSON                 | `crates/parish-npc/src/ticks.rs:853`                                     |
| NPC arrival reactions      | Reaction   | Direct call (bypass queue) | Optional  | Plain text, ≤100 tok | `crates/parish-npc/src/reactions.rs:876`                                 |
| Player intent parsing      | Intent     | Direct call (bypass queue) | No        | JSON                 | `crates/parish-tauri/src/commands.rs:495-503`                            |

Queue-based calls compete for the single in-flight worker slot. Direct-category calls run concurrently on their own per-category `OpenAiClient` instances, limited only by each provider's HTTP connection pool. Effective parallelism is therefore `1 (worker) + N (direct-category clients, one per Intent/Reaction call in flight)`.

Reaction timeouts are caller-supplied (the `reactions.rs` helper takes `timeout_secs: u64`), not hardcoded on the queue side.

### Request shape (json_schema, cancel-token, streaming stats)

`InferenceRequest` (`crates/parish-inference/src/lib.rs`) carries optional shape and lifecycle controls in addition to the prompt:

| Field         | Type                           | Purpose                                                                                                                                                                                      |
| ------------- | ------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `json_mode`   | `bool`                         | Sends `response_format: {"type":"json_object"}` — loose JSON, no enforced shape                                                                                                              |
| `json_schema` | `Option<JsonSchemaSpec>`       | Sends `response_format: {"type":"json_schema", "json_schema": {...}}` — enforced shape via constrained decode. **Wins over `json_mode` when both are set.**                                  |
| `cancel`      | `Option<CancellationToken>`    | `tokio_util::sync::CancellationToken`; firing it races the in-flight future via `tokio::select!` and drops the connection. Tested end-to-end: vllm-mlx frees the slot in 1-30 ms post-cancel |
| `token_tx`    | `Option<mpsc::Sender<String>>` | When `Some`, the worker uses streaming generate and forwards tokens via a proxy channel that also records `ttft_ms` + `output_tokens`                                                        |
| `max_tokens`  | `Option<u32>`                  | Hard cap on output. Strongly recommended when streaming reasoning models or pairing with `cancel`                                                                                            |

`InferenceQueue` has three send methods of progressively wider surface:

- `send(prompt, ...)` — legacy path, no schema, no cancel
- `send_with_schema(...)` — adds `json_schema`
- `send_full(...)` — adds `cancel` on top

Worker captures streaming stats via `StreamStats { ttft, tokens }` and records them on the `InferenceLogEntry` for debug-panel display.

### Constrained-decode trade-offs

`response_format: json_schema` is the production path for structured outputs (Intent, Tier 2 Simulation), enforced by the underlying engine (vllm-mlx uses its constrained sampler). Two caveats:

1. **Decode is ~2.2x slower** than free generation on gemma-3-4b-it-4bit (44 vs 95 tok/s). Affordable at small outputs (≤80 tokens), budget-breaking past that. Schema design should keep required fields small and prefer flat shapes over arrays-of-objects.
2. **Array item shape must be declared explicitly.** Tier2's `mood_changes` / `relationship_changes` are typed as `{type: array}` with no item schema today. The constrained decoder doesn't enforce item shape, and observed outputs sometimes return arrays of `{"summary": "..."}` or other off-shape objects that break Rust deserialization. Tighten with `items: {type: object, properties: {...}, required: [...]}` when revisiting these schemas.

## Throughput Estimates

### Linux + Ollama (ADR-005 baseline, RX 9070)

- 9B-class local model (Ollama, q4) on RX 9070: **~40-60 tokens/sec**
- At ~100-150 tokens per NPC response: **~3-6 NPC "thoughts" per second**

### Cloud providers

- Claude Sonnet 4.6, Gemini 2.5 Flash: faster per-token than local but add ~300-1000 ms network round-trip
- Budget ~1-2 s per Tier 1 response end-to-end

### macOS / Apple Silicon + vllm-mlx

Measured May 2026 on a single-model loadout, `mlx-community/gemma-3-4b-it-4bit`, vllm-mlx 0.3.x, M-series unified memory. **Production-faithful refresh**: bench prompts mirror `INTENT_SYSTEM_PROMPT` / `build_reaction_prompt` / `build_tier2_prompt` / `build_tier3_prompt` byte-for-byte; `max_tokens` caps match production (Reaction 100, Tier 2 Sim 200, Tier 3 Batch 600). See the local archive at `docs/proofs/local-perf/evidence.md` for raw data and methodology; below is the design-relevant summary.

| Category         | ttft p50      | total p50    | total p95 | budget               | verdict                                                                              |
| ---------------- | ------------- | ------------ | --------- | -------------------- | ------------------------------------------------------------------------------------ |
| Intent           | 61 ms         | 451 ms       | 734 ms    | ttft<200 / total<500 | FAIL p95 (1B intent slot is the fix)                                                 |
| Reaction         | 33 ms         | 147 ms       | 1127 ms   | ttft<400 / total<800 | FAIL p95 (bimodal; first-meeting introductions saturate the 100-tok cap at ~1060 ms) |
| **Tier 2 Sim**   | 46 ms         | 1089 ms      | 1095 ms   | total<1500           | **PASS**                                                                             |
| **Tier 3 Batch** | 144 ms        | **30459 ms** | 30667 ms  | total<1500 (wrong)   | **FAIL by 20x** — needs its own budget on the Batch lane                             |
| Dialogue         | 1.1 ms cached | —            | —         | ttft<1000            | **PASS** (prefix cache delivers)                                                     |

Tier 3 deserves its own row because the 6-NPC batch produces ~600 tokens (hitting the production cap) and the constrained decoder runs at ~20 tok/s — 30 s/batch on this engine. Options: smaller batch size, drop schema, or cloud-route Tier 3 (Gemini Flash-Lite handles this in <2 s). See evidence.md for the full breakdown.

Key engine properties on this path:

- **Prefix-cache delivers**: identical-prefix requests get **~1.1-1.4 ms cached ttft** (verified across a 6-turn game-loop sequence sharing the same system prompt).
- **Continuous batching is a free lunch**: three simultaneous requests (intent + reaction + sim) finish in **~587 ms wall** — less than two sequential. The "two-worker concurrency" goal is largely already delivered; the remaining engineering is firing requests concurrently from one queue rather than spinning a second worker.
- **Cold-load**: `vllm-mlx serve` spawn → first 200 OK = **~3.3 s** with persisted prefix cache; RSS **~4.3 GB** for the 4B 4-bit model.
- **Cancel-token works end-to-end**: cancelling a streaming request mid-decode and immediately firing a new one yields post-cancel ttft of **1-30 ms** — vllm-mlx frees the slot promptly.
- **Schema-enforcement tax**: constrained `response_format: json_schema` decode is **~2.2x slower** than free generation (44 vs 95 tok/s on gemma-3-4b-it-4bit). At ≤80 output tokens, comfortably absorbed.
- **Sim eventfulness ceiling**: prompts that legitimately emit non-empty `mood_changes` / `relationship_changes` arrays (a fight, a death) blow past the 1500 ms budget at ~4.2 s p50. Mitigation paths documented in evidence.md.

### Known broken paths

- **`mlx-community/gemma-3-1b-it-4bit`** does not load on vllm-mlx 0.3.x — `mlx_vlm.speculative.drafters.gemma3_text` module missing despite `mlx_lm/models/gemma3_text.py` existing in the same install. Blocks the two-slot loadout via gemma family.
- **vllm-mlx `--models-config` (multi-model registry)** has a `pydantic ValidationError: model field None` on the response builder even when the model loads. Blocks single-process two-model serving on this version.
- **Rapid-MLX 0.6.30** (vllm-mlx fork): every `mlx-community/gemma-3-4b-it-4bit` request hangs (`stream=false` 60 s no chunks; `stream=true` first chunk in 948 μs then hangs). Their roadmap acknowledges "VLM pipeline overhead" on Gemma 3 as a known issue. **Sidelined**, revisit when (a) we move to Qwen3.5 family, (b) we add tool-calling, (c) Rapid lands EAGLE-3, or (d) they fix the gemma-3 VLM-pipeline routing.

Numbers will vary with model, quantization, and prompt length — measure on your own hardware before tuning tick intervals.

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

| Category   | Purpose                                           | Default                                 |
| ---------- | ------------------------------------------------- | --------------------------------------- |
| Dialogue   | Player-facing NPC conversation (Tier 1)           | Cloud if configured, else base provider |
| Simulation | Background NPC sim (Tier 2 + Tier 3 batch)        | Base provider (usually local)           |
| Intent     | Player input classification (direct, low-latency) | Base provider (usually local)           |
| Reaction   | NPC arrival greetings (direct, short timeout)     | Base provider (usually local)           |

Configuration is runtime-mutable via `/provider`, `/model`, `/key`, and `/cloud` commands. Changing provider settings respawns the inference worker with a new client and swaps per-category clients atomically.

### Candidate Models and Promotion Status

> Specific candidates drift as the open-model landscape evolves. A candidate
> is not a production recommendation until the frozen holdout, hard-failure,
> reliability, latency, and memory gates in `promptfoo/` produce a passing
> receipt. As of July 2026, no fully local dialogue profile is qualified.

#### Linux / Windows candidates (RX 9070 16 GB + i9-13900KS)

| Category                   | Local pick                              | Cloud pick                | Why                                                                                                                  |
| -------------------------- | --------------------------------------- | ------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| Dialogue                   | Gemma 4 9B or Qwen 3.5 9B (unqualified) | Claude Sonnet 4.6         | Quality-critical; candidates must pass the promotion gate before setup may call them qualified                       |
| Simulation (Tier 2 nearby) | Qwen 3.5 9B                             | Gemini 2.5 Flash          | Structured JSON throughput matters more than prose quality                                                           |
| Simulation (Tier 3 batch)  | Qwen 3.5 9B                             | **Gemini 2.5 Flash-Lite** | $0.10 / $0.40 per 1M tokens makes cloud Tier 3 effectively free at game scale; stack with batch API + prompt caching |
| Intent                     | Ministral 3 3B                          | — (always local)          | Low-latency JSON / function-calling; 3B is enough and keeps the player's input path private                          |
| Reaction                   | Ministral 3 3B                          | Gemini 2.5 Flash-Lite     | Short, fast responses; shares the 3B model with Intent                                                               |

#### macOS / Apple Silicon + vllm-mlx (measured May 2026, M-series unified memory)

Legacy two-slot Qwen candidate (experimental; not production-qualified):

| Category   | Local pick                                              | Cloud pick            | Notes                                                                                                                                  |
| ---------- | ------------------------------------------------------- | --------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| Dialogue   | `mlx-community/Qwen2.5-14B-Instruct-4bit` (slot :8000)  | Claude Sonnet 4.6     | Dialogue ttft p50 128 ms / p95 367 ms, total p95 2377 ms, 17.5 tok/s. Opus-blind quality 4.76/5, 0% script-flaw on the 100-prompt scan |
| Simulation | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` (slot :8001) | Gemini 2.5 Flash      | PASS on Tier 2 (~3x faster than gemma-3-4b). Tier 3 still over the 1500 ms budget but ~3x improved — Tier 3 is intentionally relaxed   |
| Reaction   | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` (slot :8001) | Gemini 2.5 Flash-Lite | PASS                                                                                                                                   |
| Intent     | `mlx-community/Qwen2.5-1.5B-Instruct-4bit` (slot :8001) | — (always local)      | PASS — small Qwen unblocks the previously-failing Intent path                                                                          |

Both models load through vllm-mlx 0.3.x's clean mlx_lm path
(`mllm=False`) — neither matches the MLLM pattern that traps gemma-3.
Memory footprint ~9.3 GB resident total (1.3 GB + 8 GB).
The older May 2026 measurements remain useful historical baselines, but they
predate the production-prompt holdout, hard-failure, reliability, and
player-ready-turn gate. The current setup therefore recommends BYOK cloud
(OpenRouter / Anthropic / Google) for dialogue at every memory tier and labels
the local profile experimental. Below 16 GB, the small-slot-only fallback also
produces flat, anachronistic dialogue (Opus-blind 2.96/5). See the “Qwen
two-slot validation (May 2026)” section of the local archive at
`docs/proofs/local-perf/evidence.md` and the May 2026 Opus-blind comparison at
`docs/proofs/local-perf/quality_eval_20260511T163000Z.md`.

The 7B tier was the prior Dialogue pick. With the sprinkle-only
`language_directive` patch the 14B → 7B Overall quality gap is only
0.36 (about judge-noise), so the case for 7B as a separate tier is
weak — pick 14B when host memory permits, drop to 1.5B otherwise.

Legacy single-slot fallback (gemma-3-4b on one process), kept for
reference: Dialogue PASS, Reaction PASS, Sim Tier 2 PASS, Intent FAIL
(229 ms+ ttft on shared 4B vs 200 ms budget), Tier 3 FAIL (30 s wall).

Notes on the picks:

- **Gemma 4** (Apache 2.0, April 2, 2026) tends to be stronger at naturalistic prose. **Qwen 3.5 9B** (Feb 2026) tends to be stronger at structured output. Qwen 3.5 does not ship a 14B size — 9B is the new Tier 1 target, superseding the Qwen3 14B reference from ADR-005.
- **Ministral 3 3B** ships with first-class JSON / function-calling, which is exactly what Intent and Reaction need.
- **Claude Sonnet 4.6** remains the quality leader for in-character dialogue if you have a cloud budget.
- **gemma-3-4b-it-4bit** on macOS is the measured production target; the 1b text-only variant doesn't load on current vllm-mlx (mlx_vlm gemma3_text drafter missing).
- **Reasoning models (Qwen3.5, DeepSeek-R1, etc.)** stall on unconstrained generation — `<think>` blocks burn the budget. If we ever use one, every category must pass `response_format` to short-circuit thinking into JSON. Today, Reaction and Dialogue default to free-form prose, so they'd break. Policy: pin gemma-3 family for local on macOS until reasoning-aware routing exists.
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

**Fully-local** — zero cloud dependency; run two Ollama instances on different ports so the larger model stays loaded for Dialogue/Simulation while the 3B handles Intent/Reaction. The engine's built-in auto-selector picks a gemma4 tier based on VRAM / unified memory (see `select_model_for_vram` in `crates/parish-setup/src/model_select.rs`); override here if you want something different:

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

**Apple Silicon local (macOS, MLX engine)** — two vllm-mlx processes, one per slot. Auto-launch is wired via `VllmMlxProcess::ensure_running` (`crates/parish-setup/src/process.rs`); set `VLLM_MLX_BIN` to override the binary path when rapid-mlx or another installer has clobbered the `~/.local/bin/vllm-mlx` symlink.

```toml
[provider]
name = "vllm-mlx"
base_url = "http://localhost:8000"
model = "mlx-community/Qwen2.5-14B-Instruct-4bit"

[provider.intent]
base_url = "http://localhost:8001"
model = "mlx-community/Qwen2.5-1.5B-Instruct-4bit"

[provider.reaction]
base_url = "http://localhost:8001"
model = "mlx-community/Qwen2.5-1.5B-Instruct-4bit"

[provider.simulation]
base_url = "http://localhost:8001"
model = "mlx-community/Qwen2.5-1.5B-Instruct-4bit"
```

Pre-launch:

```sh
uv tool install vllm-mlx
# or: pip install vllm-mlx
# verify pristine binary if you also have rapid-mlx installed:
# readlink -f ~/.local/bin/vllm-mlx

# Start two slots manually (multi-slot auto-launch is TODO):
vllm-mlx serve mlx-community/Qwen2.5-14B-Instruct-4bit \
    --port 8000 --enable-prefix-cache --continuous-batching &
vllm-mlx serve mlx-community/Qwen2.5-1.5B-Instruct-4bit \
    --port 8001 --enable-prefix-cache --continuous-batching &
```

The engine auto-spawns the _base_ `vllm-mlx serve <model> --port 8000` if nothing is reachable, and stops it on shutdown. Cold-load is ~3.3 s with persisted prefix cache per process. Total memory ~9.3 GB resident across both processes. Single-slot fallback (one process, one model) works for hosts with tighter memory — point base at the 1.5B and drop the per-category overrides.

Legacy single-slot config:

```toml
[provider]
name = "vllm-mlx"
base_url = "http://localhost:8000"
model = "mlx-community/gemma-3-4b-it-4bit"
```

If the host can't hold the 14B (< 16 GB unified memory), point the
whole loadout at a BYOK cloud endpoint instead of degrading the local
tier — the small-slot-only fallback would compromise dialogue too far
(Opus-blind 2.96/5). Example with OpenRouter:

```toml
[provider]
name = "openrouter"
model = "anthropic/claude-sonnet-4-6"
api_key = "$OPENROUTER_API_KEY"
```

Or pin per-category via Anthropic's tiered Opus / Sonnet / Haiku
preset — see the "Cloud-light" Starter Configuration above.

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

```text
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

| Condition                                           | Response to player                                                                                    |
| --------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| No NPC at current location, queue absent or present | Random idle-world flavour message                                                                     |
| NPC present, but `inference_queue` is `None`        | Clear message: "There's someone here, but the LLM is not configured — set a provider with /provider." |

This prevents the confusing case where the player tries to speak to a character and receives a "wind stirs" message with no indication that the LLM is unconfigured.

### Background Tasks

Two fire-and-forget tasks are spawned in `spawn_background_ticks()`:

| Task       | Interval | Purpose                                                 |
| ---------- | -------- | ------------------------------------------------------- |
| World tick | 5 s      | Broadcasts `world-update` snapshot; ticks NPC schedules |
| Theme tick | 500 ms   | Broadcasts `theme-update` palette                       |

Both log `tracing::debug!` at startup. Serialisation errors inside either loop surface via `EventBus::emit()`'s warn logging. The Tokio runtime logs task panics automatically; no additional panic wrappers are used.

## Related

- [NPC System](npc-system.md) — NPC context construction feeds the inference queue
- [Cognitive LOD](cognitive-lod.md) — Tier determines model selection and batch strategy
- [Player Input](player-input.md) — Natural language input parsed via this pipeline
- [Debug UI](debug-ui.md) — Debug panel that displays inference call log
- Local archive `docs/proofs/local-perf/evidence.md` — raw measurements for the macOS / vllm-mlx path, the four-runtime benchmark, and the corrected vllm-mlx-doesn't-hang finding
- [ADR 005: Ollama Local Inference](../adr/005-ollama-local-inference.md)
- [ADR 008: Structured JSON LLM Output](../adr/008-structured-json-llm-output.md)

## Source Modules

- [`parish-inference`](../../parish/crates/parish-inference/src/) — inference queue, worker, validation, and logs
- [`parish-diagnostics/debug_snapshot`](../../parish/crates/parish-diagnostics/src/debug_snapshot/) — `InferenceLogEntry`, `InferenceDebug` structs (re-exported as `parish_core::debug_snapshot`)
- [`parish-providers`](../../parish/crates/parish-providers/src/) — provider HTTP clients and simulator/mock backends
- [`parish-input`](../../parish/crates/parish-input/src/) — Player input parsing
- [`parish-npc`](../../parish/crates/parish-npc/src/) — NPC context construction
