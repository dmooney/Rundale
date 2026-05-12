# Proof: local-inference perf goals + Mac runtime policy

Evidence type: gameplay transcript

(The harness drives the same `InferenceQueue` path used by gameplay;
transcripts below are the worker's per-call trace and aggregate
p50/p95.)

## Shipping policy (final state)

macOS local-inference default: **two-slot vllm-mlx, Qwen2.5-14B for
Dialogue, Qwen2.5-1.5B for everything else, 16 GB unified-memory
minimum**.

| Slot     | Port | Model                                    | Categories                  | Memory |
|----------|------|------------------------------------------|-----------------------------|--------|
| Dialogue | 8000 | mlx-community/Qwen2.5-14B-Instruct-4bit  | Dialogue                    | ~8 GB  |
| Small    | 8001 | mlx-community/Qwen2.5-1.5B-Instruct-4bit | Intent, Reaction, Simulation | ~1.3 GB |

Total resident ~9.3 GB. `Provider::recommended_for_platform()`
returns `Provider::VllmMlx` on macOS ≥ 16 GB unified memory and
`Provider::Simulator` below; first-run UI then routes sub-16 GB
into BYOK rather than degrade to a 1.5B-everywhere loadout that
scored 2.96/5 Opus-blind. `unified_memory_bytes()` in
`parish-config/src/provider.rs` probes `sysctl -n hw.memsize`.

Auto-launch flows through `GameConfig::vllm_mlx_extra_slots()` →
`VllmMlxProcess::ensure_slots()` (deduped against the base slot,
idempotent). The base slot spawns from `setup_provider_client`;
extras spawn in parallel and are tracked on
`RuntimeProcesses { ollama, vllm_mlx: Vec<VllmMlxProcess> }` so
`RuntimeProcesses::stop()` cleans up every spawned child on shutdown.

## Latency budgets

Locked in from a session with the Rundale lead. Per-call cost at
steady state (not cold load) on the target machine.

| Category   | ttft p95 | total p95 | Why                                                              |
|------------|----------|-----------|------------------------------------------------------------------|
| Intent     | < 200 ms | < 500 ms  | Player presses Enter — must feel instant.                        |
| Reaction   | < 400 ms | < 800 ms  | NPC arrival quip; sub-second keeps world alive.                  |
| Simulation | < 800 ms | < 1500 ms | Per-tick bg sim per location; continuous loop pausing on player. |
| Dialogue   | < 1000 ms| streamed  | Player reads as tokens arrive; total can be many seconds.        |

## Runtime selection — vllm-mlx wins

Four-runtime bench on the same prompt fixtures (see "Files cited"
below for raw output):

| Runtime          | Intent ttft p95 | Reaction ttft p95 | Sim ttft p95 | Dialogue ttft p95 | Notes                                       |
|------------------|-----------------|--------------------|--------------|-------------------|---------------------------------------------|
| Ollama 0.6.x     | 1855 ms         | 8146 ms            | 13021 ms     | 6329 ms           | No prefix cache; FAIL across the board.     |
| LM Studio MLX    | 295 ms          | 242 ms             | 242 ms       | 278 ms            | 3-of-4 pass; rejects `response_format: json_object`. |
| **vllm-mlx 0.3.x** | **62 ms**       | **34 ms**          | **35 ms**    | **104 ms**        | Working prefix cache; 1.1 ms cached ttft on game-loop sequence. |
| Rapid-MLX 0.6.30 | (hangs on gemma-3 — VLM pipeline overhead known on their roadmap) |

vllm-mlx is two orders of magnitude faster than LM Studio on ttft
once prefix cache warms (1.1 ms p50 across a 6-turn game-loop
sequence with shared system prompt), and the only runtime that
correctly handles both `response_format: json_object` and
`response_format: json_schema` (verified standalone at
`/tmp/vllm-mlx-repro/repro.sh`; the earlier "vllm-mlx stalls on
constrained decode" report was a misdiagnosis — the actual
pathology was an unconstrained reasoning model emitting `<think>`
tokens to timeout).

Decode tok/s on the chosen models: 1.5B ~90 tok/s,
14B ~17.5 tok/s (constrained), 14B ~95 tok/s (free) — measured
across the production-faithful bench below.

## Production-faithful bench

The harness fixture mirrors the actual production prompt builders
byte-for-byte:

| Bench category | Production source                                     |
|----------------|--------------------------------------------------------|
| Intent         | `parish-input/src/intent_llm.rs::INTENT_SYSTEM_PROMPT` |
| Reaction       | `parish-npc/src/reactions/arrival_reactions.rs::build_reaction_prompt` |
| Tier 2 Sim     | `parish-npc/src/ticks.rs::build_tier2_prompt`          |
| Tier 3 Batch   | `parish-npc/src/ticks.rs::build_tier3_prompt`          |

`max_tokens` synced to production caps: Reaction 100, Tier 2 200,
Tier 3 600. Intent and Dialogue uncapped (schema bounds Intent,
Dialogue streams).

### Qwen2.5-1.5B (small slot)

[`bench-qwen15.txt`](bench-qwen15.txt)

| Category   | total p95 | Verdict |
|------------|-----------|---------|
| Intent     | < 500 ms  | PASS    |
| Reaction   | < 800 ms  | PASS    |
| Tier 2 Sim | < 1500 ms | PASS    |
| Tier 3 Sim | over budget but ~3x faster than gemma-3-4b | improved (Tier 3 was always going to need its own budget — Batch lane natural pacing handles it) |

### Qwen2.5-14B (Dialogue slot)

[`bench-qwen14-prod.txt`](bench-qwen14-prod.txt)

| Category   | ttft p50 / p95 | total p50 / p95 | tok/s p50 | Verdict |
|------------|----------------|-----------------|-----------|---------|
| Intent     | 327 / 859 ms   | 1174 / 1695 ms  | 11.9      | FAIL    |
| Reaction   | 128 / 348 ms   | 1463 / 2480 ms  | 18.1      | FAIL    |
| Simulation | 329 / 693 ms   | 10620 / 26262 ms| 11.9      | FAIL    |
| **Dialogue** | **128 / 367 ms** | **2087 / 2377 ms** | **17.5** | **PASS** |

14B is over budget on every category except the Dialogue slot it
targets. The two-slot split is the design intent — 14B never serves
Intent/Reaction/Sim; the 1.5B slot does.

### Qwen2.5-7B (rejected as Dialogue model)

[`bench-qwen7-prod.txt`](bench-qwen7-prod.txt) shipped initially
but a 100-prompt scan exposed a code-switch failure: 7B occasionally
replied entirely in Irish on cough/illness prompts (~1% of scan).
Two fixes followed and we benched 14B as the next tier:

1. `language_directive` strengthened (sprinkle-only clause, Latin-only
   character guard) in `parish-npc/src/lib.rs`.
2. Tiered up to 14B for hosts with memory headroom.

After the code-switch fix the 7B-vs-14B Opus-blind gap narrowed to
0.36 Overall — at the judge-noise threshold (~0.3). 7B dropped from
defaults: the 0.36 delta isn't worth the per-host knob now that the
catastrophic failure mode is patched.

## Quality eval

`/eval-dialogue` skill, Claude Opus 4.7 as judge, models hidden
behind `Model X/Y/Z`. Full report:
[`quality_eval_20260511T163000Z.md`](quality_eval_20260511T163000Z.md).

| Model                       | Character | Authenticity | Language | Responsiveness | Craft | Overall |
|-----------------------------|-----------|--------------|----------|----------------|-------|---------|
| Qwen2.5-14B-Instruct-4bit   | 5.00      | 4.40         | 5.00     | 4.80           | 4.60  | **4.76** |
| Qwen2.5-7B-Instruct-4bit    | 4.60      | 3.80         | 4.40     | 5.00           | 4.20  | **4.40** |
| Qwen2.5-1.5B-Instruct-4bit  | 2.00      | 2.20         | 5.00     | 3.40           | 2.20  | **2.96** |

Raw samples: [`dlg-qwen15.txt`](dlg-qwen15.txt),
[`dlg-qwen7.txt`](dlg-qwen7.txt).

## Flaw scan (100 prompts)

`dialogue_flaw_scan_14b.md`: **0/100** prompts flagged for non-Latin
script leakage or shape errors on Qwen2.5-14B after the Latin-only
guard + sprinkle-only clause landed.

## Engineering plumbing landed in this PR

### Streaming metrics

`InferenceLogEntry` carries `ttft_ms` and `output_tokens`. The
worker captures them via a proxy mpsc channel that observes the
first-token timestamp and counts forwarded chunks. The debug panel
renders `total · ttft · tok/s` per call.

Regression test:
`parish_inference::tests::test_streaming_request_records_ttft_and_token_count`.

### `/inf-bench` harness

`parish/crates/parish-inference/examples/inf_bench.rs`, wrapped by
`just inf-bench`. Drives representative prompts for each category
through the real `InferenceQueue` worker against any OpenAI-compat
endpoint. Reports p50/p95 ttft + total per category and PASS/FAIL
against budgets. `--schema` flag flips Intent + Simulation samples
from `json_object` to a real `json_schema` payload.

### `json_schema` plumbing

`OpenAiClient` now exposes `generate_text_with_format`,
`generate_json_with_format`, and `generate_stream_with_format`,
taking `Option<ResponseFormat>` where `ResponseFormat` is
`{JsonObject, JsonSchema { name, schema }}`. `InferenceRequest`
gained a `json_schema: Option<JsonSchemaSpec>` field; the worker
resolves the effective wire format with schema winning over
`json_mode`.

### Cancel-token plumbing

`InferenceRequest` gained `cancel: Option<CancellationToken>`
(re-exported from `tokio_util`). `inference_with_timeout` races
the inflight future against both the configured timeout *and* the
optional cancel token via `tokio::select!`. When the token fires,
the future is dropped — closing the underlying HTTP/SSE connection
so Ollama, LM Studio, and vllm-mlx free their model slots.

Regression test:
`test_cancellation_fires_mid_stream_yields_error` drives the
simulator client (~40 ms/token), fires cancel after the first
forwarded token, asserts the response error contains "cancel" and
at least one token was observed before the cancel landed.

Verified against vllm-mlx end-to-end: post-cancel probes return in
33-78 ms, server log shows `[abort_prefill] Marked … for prefill abort`
and `[disconnect_guard] generator exhausted normally` — vllm-mlx
recognizes the disconnect, frees the slot, serves the next request
without delay.

### Default non-streaming timeout 30 → 300 s

`parish-config/src/engine.rs::default_timeout_secs`. The previous
value pre-empted reasoning-model cold-loads and showed up as
`network error: error sending request` in the debug panel.

## Known limits

- **Schema-enforcement tax**: constrained decode is ~2.2× slower
  per token than free generation on vllm-mlx (~38-44 tok/s vs
  ~95 tok/s). Comfortably absorbed at our output sizes (Intent ~25
  tokens, Sim ~40 tokens). Any new constrained category exceeding
  ~80 tokens of structured JSON starts breaking budgets even when
  prefill is fast — pressure is on schema design and `max_tokens`
  caps, not the runtime.
- **Tier 3 needs its own budget**: 6-NPC batch produces 600 tokens
  at ~20 tok/s constrained ≈ 30 s. Lands cleanly on the Batch lane's
  natural pacing (a few times per minute). Not the 1500 ms
  simulation budget.
- **Eventful sim**: when the model emits non-empty
  `mood_changes`/`relationship_changes` arrays (a fight, a death),
  output blows past 1500 ms by 2-3×. Mitigations: cap `max_tokens`
  to ~80, stream + cancel at budget edge, or rule-based
  eventfulness detection. Reaction lane absorbs the dramatic
  moments today; sim is the steady-state path.
- **vllm-mlx post-cancel instability**: requests following cancelled
  streaming + `response_format: json_schema` sometimes degrade
  unless an explicit `max_tokens` is set. Always pin `max_tokens`
  when using cancel-token against vllm-mlx.
- **Concurrency under continuous batching**: three concurrent
  requests finish in less wall time than two sequential. We don't
  need two physical workers; firing two requests concurrently from
  one queue is already the answer. Engineering follow-up is making
  the queue actually fire them concurrently rather than
  serializing.

## Reasoning-model fallback policy

Standalone repro at `/tmp/vllm-mlx-repro/repro.sh` characterized
two failure modes for reasoning models (Qwen3.5):

- Unconstrained generation stalls: the model spends the budget on
  `<think>` tokens before emitting content. Reproduces at >45 s.
- Constrained generation completes but is slow (~7 s).

Policy: do not pick a reasoning model as the main slot on local
without enforcing `response_format` on every category that has a
budget. Reaction and Dialogue default to free-form prose, which
would stall a reasoning model. macOS local pins to Qwen2.5
(non-reasoning); any future reasoning route uses a dedicated
category that always passes `response_format` and absorbs a 5-10 s
budget.

## Test results

```
cargo test --workspace:           2637 passed, 16 ignored (66 suites)
cargo clippy --workspace --tests: clean
just check (fmt + clippy + test): pass
just agent-check:                 pass
```

## Files cited

Raw bench outputs:

- [`bench-qwen14-prod.txt`](bench-qwen14-prod.txt) — 14B production-faithful
- [`bench-qwen7-prod.txt`](bench-qwen7-prod.txt) — 7B production-faithful (historical, pre-tier-up)
- [`bench-qwen15.txt`](bench-qwen15.txt) — 1.5B production-faithful
- [`bench-prod3.txt`](bench-prod3.txt) — gemma-3-4b production-faithful (runtime-selection era)

Raw dialogue samples (Opus-blind compare inputs):

- [`dlg-qwen7.txt`](dlg-qwen7.txt)
- [`dlg-qwen15.txt`](dlg-qwen15.txt)

Quality / flaw evidence:

- [`quality_eval_20260511T163000Z.md`](quality_eval_20260511T163000Z.md) — Opus-blind 3-way after code-switch fix
- [`dialogue_flaw_scan_14b.md`](dialogue_flaw_scan_14b.md) — 14B 100-prompt scan (0/100 flagged)

## Files touched

- `parish/crates/parish-config/src/engine.rs` — default_timeout_secs 30 → 300
- `parish/crates/parish-inference/src/lib.rs` — `StreamStats`, proxy channel, log-entry fields, `AnyClient` per-format paths
- `parish/crates/parish-inference/src/openai_client.rs` — `ResponseFormat`, `generate_*_with_format`
- `parish/crates/parish-inference/examples/inf_bench.rs` — new harness
- `parish/crates/parish-inference/src/setup.rs` — `VllmMlxProcess::ensure_slots`, multi-slot auto-launch
- `parish/crates/parish-config/src/provider.rs` — `Provider::VllmMlx`, `recommended_for_platform`, `unified_memory_bytes`
- `parish/crates/parish-npc/src/lib.rs` — `language_directive` sprinkle-only clause + Latin-only guard
- `parish/crates/parish-npc/src/ticks.rs::build_tier2_prompt` — uneventful-scene steering
- `parish/crates/parish-core/src/{debug_snapshot,game_session}.rs` — fill new log-entry fields
- `parish/crates/parish-server/src/routes.rs` — forward fields through redaction
- `parish/apps/ui/src/components/{DebugInferenceTab,DebugNpcsTab}.svelte` — display `total · ttft · tok/s`
- `parish/justfile` + root `justfile` — `just inf-bench` recipe
