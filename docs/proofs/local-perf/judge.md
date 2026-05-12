# Judge verdict — local-inference perf + Mac runtime policy

Verdict: sufficient

Technical debt: clear

The PR ships measurable, instrumented work behind real
implementations across four buckets: streaming metrics + bench
harness, runtime selection (vllm-mlx over Ollama / LM Studio /
Rapid-MLX), model selection (Qwen2.5 14B Dialogue + 1.5B small slot,
16 GB minimum), and the `json_schema` + cancel-token plumbing the
rest of the system relies on.

## What was claimed and verified

1. **Streaming metrics** (`ttft_ms`, `output_tokens`, tok/s) plumbed
   through the inference worker, surfaced on `InferenceLogEntry`,
   and rendered in the debug panel. Regression test pins it.
2. **`/inf-bench` harness** drives representative per-category
   prompts through the real worker against any OpenAI-compat
   endpoint and reports PASS/FAIL against latency budgets, with
   `--schema` opt-in for real `json_schema` payloads.
3. **Runtime selection**: vllm-mlx beats Ollama by ~50× on prefix-
   cache-bound ttft and beats LM Studio by ~3× on the same; correct
   handling of `response_format: json_object` and `json_schema`
   verified by standalone repro. Rapid-MLX hangs on gemma-3 today
   (VLM pipeline overhead, acknowledged on their roadmap).
4. **Model selection**: Qwen2.5-14B for Dialogue (Opus-blind 4.76/5;
   0/100 flaw-scan after Latin-only guard + sprinkle-only clause),
   Qwen2.5-1.5B for Intent/Reaction/Sim (under-budget on every
   small-output category). 7B dropped from defaults — post-fix
   delta to 14B is 0.36 Overall, at judge-noise threshold.
5. **16 GB minimum** enforced by `Provider::recommended_for_platform`
   + `unified_memory_bytes()`; below the floor first-run UI steers
   to BYOK rather than degrade to the 2.96/5 small-only fallback.
6. **`json_schema` plumbing**: `ResponseFormat` enum,
   `generate_*_with_format` paths, `InferenceRequest::json_schema`,
   `InferenceQueue::send_with_schema`. Schema wins over `json_mode`
   in the worker.
7. **Cancel-token plumbing**: `InferenceRequest::cancel`,
   `inference_with_timeout` racing the future against both timeout
   and cancel via `tokio::select!`. Verified end-to-end against
   vllm-mlx — post-cancel probes return in 33-78 ms, server log
   confirms `[abort_prefill] Marked … for prefill abort` and slot
   freed.
8. **Default non-streaming timeout 30 → 300 s** so reasoning-model
   cold-loads no longer surface as opaque "network error" entries.

## Independent verification

- `parish-inference/src/lib.rs` — `StreamStats` lives in module
  scope; captured via a proxy mpsc channel that forwards to the
  original consumer; the proxy task awaits the observer before
  reading stats so no tokens are lost. `InferenceLogEntry` fields
  are `Option<u64>` so non-streaming entries keep `None`.
  `inference_with_timeout` pins the future and races it against
  `cancel.cancelled()` and `tokio::time::sleep(timeout)` — biased
  toward cancel, so a fired cancel always wins over an in-flight
  response.
- `parish-inference/src/openai_client.rs` — `ResponseFormat` enum
  is `JsonObject | JsonSchema { json_schema: JsonSchemaSpec }`;
  `build_request` takes `Option<ResponseFormat>`; new
  `generate_*_with_format` paths covered by tests.
- `parish-inference/examples/inf_bench.rs` runs against any
  OpenAI-compat endpoint (verified: Ollama, LM Studio, vllm-mlx)
  and produces the tables in `evidence.md`. Numbers reproducible;
  the prompt fixtures cover all four categories.
- `parish-npc/src/ticks.rs::build_tier2_prompt` reworded to steer
  empty arrays on uneventful scenes; schema unchanged so
  `Tier2Response` still parses. Existing tests still pin substring
  presence and continue to pass.
- `cargo test --workspace`: 2637 passing / 16 ignored (66 suites).
  `just check` and `just agent-check` green. Clippy clean on
  workspace.

## Known limits (documented in `evidence.md`)

- **Schema-enforcement tax** ~2.2× decode slowdown on constrained
  paths. Absorbed at current output sizes; pressure is on schema
  design + `max_tokens` caps.
- **Tier 3 budget**: 6-NPC batch ≈ 30 s, lands on Batch lane
  pacing — not the 1500 ms simulation budget.
- **Eventful sim**: non-empty mood/relationship arrays blow past
  1500 ms by 2-3×; mitigations documented.
- **vllm-mlx post-cancel**: pin explicit `max_tokens` to avoid the
  degraded state after cancelled `json_schema` streams.
- **Queue serialization**: continuous batching gives free
  concurrency on the server; queue still serializes on the client.
  Engineering follow-up tracked as task #8.

## Risk

Minimal at PR scope. Proxy-channel observer adds one tokio task per
streaming request; bounded `mpsc::channel(TOKEN_CHANNEL_CAPACITY)`
matches the original sizing. Timeout bump is a relaxation, not a
constraint. Sim prompt rewrite preserves the schema so the worst
case (model ignores steering and emits non-empty arrays) is "sim
takes longer" rather than data corruption. The 16 GB gate is a
defensive default for graceful onboarding fork, not a hard runtime
ceiling.

## Approved.
