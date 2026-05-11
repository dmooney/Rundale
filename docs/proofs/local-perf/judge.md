# Judge verdict — local-perf instrumentation, four-runtime benchmarks, json_schema + cancel plumbing

Verdict: sufficient

Technical debt: clear

The PR ships measurable, instrumented work behind real implementations
across three buckets (metrics + harness, four-runtime benchmark, and
schema/cancel plumbing). The follow-up runtime swap, two-worker, and
continuous-sim tasks are tracked separately and are not deferred-from-
this-PR debt.

## What was claimed

1. Streaming metrics (`ttft_ms`, `output_tokens`, tok/s) plumbed through
   the inference worker, surfaced on `InferenceLogEntry`, and rendered
   in the debug panel.
2. A reusable `/inf-bench` harness that drives representative per-
   category prompts through the real worker against any OpenAI-compat
   endpoint and reports PASS/FAIL against latency budgets, with
   `--schema` opt-in for real `json_schema` payloads.
3. Default non-streaming timeout raised 30 s → 300 s so reasoning-model
   cold-loads no longer surface as opaque "network error" entries.
4. Four-runtime benchmark: Ollama / LM Studio / vllm-mlx / Rapid-MLX
   on the same prompts and budgets, with the documented winner
   (vllm-mlx) and reasons (working prefix cache, ~3 ms ttft).
5. `json_schema` plumbing through `OpenAiClient`,
   `AnyClient::generate_text_with_format` /
   `generate_stream_with_format`, `InferenceRequest::json_schema`,
   `InferenceQueue::send_with_schema`. Schema wins over `json_mode`
   in the worker.
6. Cancel-token plumbing: `InferenceRequest::cancel`,
   `inference_with_timeout` racing the future against both timeout
   and cancel via `tokio::select!`, `InferenceQueue::send_full`
   exposing the full schema-and-cancel surface.
7. Sim prompt rewrite: `build_tier2_prompt` reworded to steer the
   model toward empty `mood_changes` / `relationship_changes` arrays
   on uneventful scenes — schema unchanged so existing Tier2 state
   updates still flow.

## Independent verification

Reviewer confirmed:

- `parish/crates/parish-inference/src/lib.rs` — `StreamStats` lives in
  module scope; captured via a proxy mpsc channel that forwards to the
  original consumer; the proxy task awaits the observer before reading
  stats so no tokens are lost. `InferenceLogEntry` fields are
  `Option<u64>` so non-streaming entries keep `None`. `inference_with_timeout`
  pins the future and races it against `cancel.cancelled()` and
  `tokio::time::sleep(timeout)` — biased toward cancel, so a fired
  cancel always wins over an in-flight response.
- `parish/crates/parish-inference/src/openai_client.rs` — `ResponseFormat`
  enum is `JsonObject | JsonSchema { json_schema: JsonSchemaSpec }`;
  `build_request` takes `Option<ResponseFormat>`; new
  `generate_text_with_format`, `generate_json_with_format`,
  `generate_stream_with_format` paths covered by tests.
- `parish/crates/parish-inference/examples/inf_bench.rs` runs against
  any OpenAI-compat endpoint (verified: Ollama, LM Studio, vllm-mlx)
  and produces the tables reproduced in `evidence.md`. Numbers are
  reproducible; the eight prompt fixtures cover all four categories.
- `parish/crates/parish-npc/src/ticks.rs::build_tier2_prompt` —
  reworded but the only test pin is substring presence
  (`Padraig (Publican)`, `summary`, etc.), so the rewrite passes
  unchanged tests. Schema unchanged; `Tier2Response` fields still
  honored by deserialization.
- `cargo test --workspace --tests` reports 2463 passing / 8 ignored
  (47 suites). `just check` and `just agent-check` are green. Clippy
  clean on `parish-inference`, `parish-npc`, and the workspace.
- The 30 → 300 s default touches one numeric literal plus two test
  assertions (`engine::tests::test_engine_config_default`,
  `test_engine_config_deserialize_empty`). No behavioural change for
  cloud providers that respond promptly.
- UI display fix: literal `·` in `DebugInferenceTab.svelte` and
  `DebugNpcsTab.svelte` replaced with the actual `·` glyph; svelte-check
  clean. Vitest fixtures updated for the two new optional fields.

## What this PR does not promise

- Does not yet meet all per-category budgets. The benchmark numbers
  (vllm-mlx + gemma-3-4b-it-4bit) PASS Reaction, Simulation, and
  Dialogue, but FAIL Intent on p95 (688 ms vs 500 ms budget) — fixable
  by the 1 B intent slot, which is a follow-up.
- Does not auto-launch vllm-mlx on macOS. Today the user must run
  `vllm-mlx serve` themselves. Tracked separately.
- Does not change category routing or worker concurrency. Single-
  flight worker is preserved; two-worker concurrency is a follow-up.
- Does not solve eventful-scene sim latency. When the model emits
  non-empty `mood_changes`/`relationship_changes` (a fight, a death),
  output blows past the 1500 ms sim budget by 2-3x. Documented as a
  known gap with concrete mitigations to apply later.

## Risk

Minimal at PR scope. The proxy-channel observer adds one tokio task
per streaming request; bounded `mpsc::channel(TOKEN_CHANNEL_CAPACITY)`
matches the original sizing. Timeout bump is a relaxation, not a
constraint, so it cannot cause new aborts. The sim prompt rewrite is
the only behavior change in production gameplay code, and it preserves
the schema so the worst case (model ignores the steering and emits
non-empty arrays) is "sim takes longer" rather than data corruption.

The follow-up gaps (Intent p95, eventful sim, runtime auto-launch,
two-worker concurrency, cancel + max_tokens pairing on vllm-mlx) are
all documented in `evidence.md` with concrete mitigation paths and
tracked as separate tasks.

## Approved.
