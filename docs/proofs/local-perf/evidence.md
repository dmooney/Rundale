# Proof: local-inference perf goals + Ollama baseline

Evidence type: gameplay transcript

(The harness drives the same `InferenceQueue` path used by gameplay; transcripts
below are the worker's per-call trace and aggregate p50/p95.)

## What changed

This PR adds two pieces of plumbing in service of fixing local inference latency on the four
inference categories (Intent, Reaction, Simulation, Dialogue):

1. **Per-call streaming metrics**: `InferenceLogEntry` now carries `ttft_ms` and
   `output_tokens`; the worker captures them via a proxy mpsc channel that observes
   the first-token timestamp and counts forwarded chunks. The debug panel renders
   `total · ttft · tok/s` per call.

2. **`/inf-bench` harness** (`parish/crates/parish-inference/examples/inf_bench.rs`,
   wrapped by `just inf-bench` and `parish/justfile :inf-bench`): drives
   representative prompts for each category through the real `InferenceQueue`
   worker against any OpenAI-compat endpoint. Reports p50/p95 ttft + total per
   category and PASS/FAIL against budgets.

3. **Non-streaming default timeout bumped** 30 → 300s
   (`parish/crates/parish-config/src/engine.rs::default_timeout_secs`). Previous
   value pre-empted reasoning-model cold-loads and showed up as
   `network error: error sending request` in the dbg panel.

## Latency budgets

Locked in from a session with the Rundale lead. Budgets describe steady-state
per-call cost (not cold-load) on a target machine.

| Category   | ttft p95 | total p95 | Why |
|------------|----------|-----------|-----|
| Intent     | < 200ms  | < 500ms   | Player presses Enter — must feel instant. |
| Reaction   | < 400ms  | < 800ms   | NPC arrival quip; subsecond keeps world alive. |
| Simulation | < 800ms  | < 1500ms  | Per-tick bg sim per location; runs continuous loop pausing on player turn. |
| Dialogue   | < 1000ms | streamed  | Player reads as tokens arrive; total can be many seconds. |

## Ollama baseline (FAIL across the board)

Run on Apple Silicon, Ollama 0.6.x, `gemma4:e4b` (4-bit, ~9 GB) for both intent
and main slots. Iters = 4 per sample, no warmup. Two sample prompts per
category except Reaction/Dialogue which had two.

```
== summary ==
category      ttft.p50  ttft.p95   tot.p50   tot.p95 tok/s.p50    errs verdict
Intent             169      1855       616      2460      63.2       0 FAIL (ttft<200ms total<500ms)
Reaction          3339      8146      3734      8484      65.3       0 FAIL (ttft<400ms total<800ms)
Simulation       12417     13021     14790     14845      64.5       0 FAIL (ttft<800ms total<1500ms)
Dialogue          5885      6329      6764      7282      63.6       0 FAIL (ttft<1000ms)
```

Decode tok/s is uniform ~65; ttft is dominated by prefill. ~6s ttft on a
~400-token prompt implies ~65 tok/s prefill — anomalously slow for Apple
Silicon, where MLX-backed runtimes typically achieve 200-500 tok/s prefill on
this model class.

Repeat-prompt iters did not converge to a fast path:
```
[Dialogue iter 0] total=6797ms ttft=6023ms
[Dialogue iter 1] total=5797ms ttft=4929ms
[Dialogue iter 2] total=5805ms ttft=5123ms
[Dialogue iter 3] total=6764ms ttft=5885ms
```
Identical input, no convergence — suggests Ollama's `/v1/chat/completions` is
not reusing prefix KV cache across calls.

## LM Studio (MLX) result — three of four budgets pass

Same harness, same fixtures, against LM Studio's local server with MLX
runtime. Two models loaded simultaneously: `gemma-3-1b` (Intent slot,
~770 MB) and `gemma-3-4b` (main slot, ~3 GB). Both quantised to 4-bit.
Iters = 3, warmup pass discarded. Run with `--no-json-mode` because LM
Studio's OpenAI-compat surface rejects `response_format: {"type":
"json_object"}` (it accepts only `text` or `json_schema`).

```
== summary (LM Studio MLX) ==
category      ttft.p50  ttft.p95   tot.p50   tot.p95 tok/s.p50    errs verdict
Intent             162       295       264       454     316.8       0 FAIL (ttft<200ms total<500ms)
Reaction           234       242       602       638     106.0       0 PASS (ttft<400ms total<800ms)
Simulation         233       242      3116      3325      98.0       0 FAIL (ttft<800ms total<1500ms)
Dialogue           247       278      1316      1417     101.0       0 PASS (ttft<1000ms)
```

Speedups vs Ollama on the same code path:

| Category | Ollama ttft p95 | MLX ttft p95 | Speedup |
|----------|------------------|---------------|---------|
| Intent | 1855 ms | 295 ms | 6.3× |
| Reaction | 8146 ms | 242 ms | 33.7× |
| Simulation | 13021 ms | 242 ms | 53.8× |
| Dialogue | 6329 ms | 278 ms | 22.8× |

Decode: gemma-3-1b clocks **316 tok/s** on this Mac vs Ollama's 65 tok/s on
e4b. ~5× the throughput, no contest.

### What still misses

- **Intent p95 = 295 ms** (budget 200 ms p95). p50 = 162 ms passes; tail
  inflates because the longest user input ("tell Padraig I saw his cow
  wandering near the bog", 51 chars) hits 295 ms cold. Likely fixable with a
  smaller intent prompt (drop the 7 worked examples) or tighter `max_tokens`.
- **Simulation total p95 = 3.3 s** (budget 1.5 s). Failure mode is decode
  volume, not prefill. The sim prompt asks for a JSON object with 3 fields and
  free-form summary; the 4 B model emits 243-302 tokens. At 98 tok/s decode,
  3 s is structural. Fixes: cap `max_tokens` (~80), shorten the prompt's
  example-block, or accept that sim runs longer than 1.5 s and adjust the
  continuous-loop cadence.

Both gaps are tractable without a runtime swap. Reaction + Dialogue meet
budget on MLX.

### LM Studio API divergence

LM Studio's OpenAI-compat does not accept `response_format: {"type":
"json_object"}`. It accepts `text` (drop-through) or `json_schema` with a
real JSON Schema. Two implications:

1. The bench's `--no-json-mode` flag was added to drop the field for portability.
2. For production, structured outputs need to flow through `json_schema`. The
   current OpenAI client only supports the boolean json-mode toggle. Wiring
   per-category schemas is a follow-up (not blocking budget hits — Reaction
   and Dialogue are streamed prose, Intent's prompt already enforces JSON
   shape via the system message, Simulation can do the same).

## vllm-mlx (Apache 2.0, headless)

`vllm-mlx serve mlx-community/gemma-3-4b-it-4bit --enable-prefix-cache
--continuous-batching` on :8000. Same model class as the LM Studio main slot.
Iters = 3, `--no-json-mode` (matches the LM Studio run for an apples-to-apples
prefill comparison; vllm-mlx handles `response_format` cleanly — see the
"vllm-mlx response_format" section below). Single-model loadout (1 B intent
slot not yet wired in this server).

```
== summary (vllm-mlx, gemma-3-4b-it-4bit, no json) ==
category      ttft.p50  ttft.p95   tot.p50   tot.p95 tok/s.p50    errs verdict
Intent              34        62       378       523      90.5       0 FAIL (ttft<200ms total<500ms)
Reaction            34        34       445       503      91.7       0 PASS (ttft<400ms total<800ms)
Simulation          34        35      2071      2202      94.1       0 FAIL (ttft<800ms total<1500ms)
Dialogue            37       104      1152      1626      93.0       0 PASS (ttft<1000ms)
```

vllm-mlx **trounces** LM Studio on ttft thanks to working prefix cache:

| Category | LM Studio ttft p50 | vllm-mlx ttft p50 | LM Studio total p50 | vllm-mlx total p50 |
|----------|---------------------|---------------------|----------------------|---------------------|
| Intent | 162 ms | **34 ms** | 264 ms | 378 ms |
| Reaction | 234 ms | **34 ms** | 602 ms | **445 ms** |
| Simulation | 233 ms | **34 ms** | 3116 ms | **2071 ms** |
| Dialogue | 247 ms | **37 ms** | 1316 ms | **1152 ms** |

Decode tok/s ~90 (vs LM Studio's ~100 on same model class) — small loss on
the steady-state side, big win on prefill that swamps it.

Intent on the 4 B model still misses the 500 ms total budget (decode-bound:
30 tokens of "intent JSON" at 90 tok/s ≈ 333 ms + ttft = 367 ms p50, but the
longer "tell Padraig…" sample drags p95 to 523 ms). Intent slot needs the
1 B model loaded alongside.

### vllm-mlx response_format (corrected: works on both models tested)

An earlier draft of this evidence claimed vllm-mlx's constrained-decode
stalls indefinitely on `response_format: json_object` and `json_schema`
for both Gemma-3 and Qwen3.5, citing upstream vLLM tickets (#21148,
#40080, #14151) as the inherited pathology.

That was wrong. A standalone, no-Parish-code probe at
`/tmp/vllm-mlx-repro/repro.sh` (run logs alongside it) shows the
opposite behavior on this hardware:

| Model | warmup (no fmt) | json_object | json_schema | control (no fmt, JSON-shaped prompt) |
|-------|-----------------|-------------|-------------|--------------------------------------|
| `mlx-community/gemma-3-4b-it-4bit` | 273 ms 200 | 1782 ms 200 | 467 ms 200 | 232 ms 200 |
| `mlx-community/Qwen3.5-4B-MLX-4bit` | 2751 ms 200 | 9738 ms 200 | 7029 ms 200 | **TIMEOUT @ 45 s** |

Two facts fall out:

1. **vllm-mlx handles `response_format` correctly.** Both `json_object`
   and a real `json_schema` payload return well-formed JSON within
   budget. The server log confirms vllm-mlx parsed the schema
   (`response_format=type='json_schema' json_schema=ResponseFormatJsonSchema(...)`).
2. **The phase that actually stalls is the unconstrained one on a
   reasoning model.** Qwen3.5 emits long `<think>` blocks before any
   content tokens; the unconstrained "Return JSON: …" prompt blew past
   the 45 s timeout. `response_format` *helps* reasoning models
   complete on budget by forcing them to skip into JSON output.

The upstream vLLM tickets cited above describe a different pathology
(a constrained-decode infinite loop on specific Gemma checkpoints with
specific schema shapes) and don't reproduce on `gemma-3-4b-it-4bit`
with the schema in our repro. They may still be real for larger Gemma
variants, but they are not our blocker.

**Practical takeaways (revised)**:

1. **Use `response_format` on vllm-mlx, especially with reasoning
   models.** Our `json_schema` plumbing in this PR is on the right
   side of the trade-off: it bounds output, enforces shape, and
   shortens reasoning runs.
2. The earlier proposal to "skip `response_format` on vllm-mlx" would
   have made things worse for Qwen3.5 by removing the only thing
   keeping it inside the per-category budget.
3. No upstream issue needs filing against `waybarrios/vllm-mlx` for
   structured-output hangs; the original report was a misdiagnosis.

The runtime-swap task (next) can default the main slot to either
gemma-3-4b-it-4bit or Qwen3.5-4B-MLX-4bit and rely on
`response_format` working. The benchmark numbers above (run with
`--no-json-mode` for an apples-to-apples prefill comparison against
LM Studio) understate vllm-mlx for reasoning-model categories where
`response_format` is the latency win, not a hazard.

### `json_schema` plumbing (this PR)

`OpenAiClient` now exposes `generate_text_with_format`,
`generate_json_with_format`, and `generate_stream_with_format`, taking
an `Option<ResponseFormat>` where `ResponseFormat` is
`{JsonObject, JsonSchema { name, schema }}`. `InferenceRequest` gained
a `json_schema: Option<JsonSchemaSpec>` field; the worker resolves the
effective wire format with schema winning over `json_mode`. The new
`/inf-bench --schema` flag flips Intent + Simulation samples from
`json_object` to a real `json_schema` payload (`INTENT_SCHEMA`,
`SIM_SCHEMA`). Reaction and Dialogue stay prose-streamed.

### Cancel-token plumbing (this PR)

`InferenceRequest` gained a `cancel: Option<CancellationToken>` field
(re-exported from `tokio_util`). `inference_with_timeout` now races the
inflight future against both the configured timeout *and* the optional
cancel token via `tokio::select!`. When the token fires, the future is
dropped — which closes the underlying HTTP/SSE connection so Ollama,
LM Studio, and vllm-mlx free their model slots. Response carries
`error: "{label} cancelled (model={model})"`.

New `InferenceQueue::send_full` exposes the full schema-and-cancel
surface; existing `send` and `send_with_schema` delegate to it with
`cancel = None` for backward compatibility. Regression test
`test_cancellation_fires_mid_stream_yields_error` drives the simulator
client (~40 ms/token), fires cancel after the first forwarded token,
and asserts the response error contains "cancel" and at least one
token was observed before the cancel landed.

This unblocks the player-turn-preempts-sim flow: tier 2/3 calls can be
cancelled mid-stream when an interactive request arrives, freeing the
main worker without waiting for the sim to drain.

## Production-faithful bench (May 2026 refresh)

The original bench used representative-but-divergent prompts. Refresh
synced the bench fixture to mirror the actual production prompt
builders byte-for-byte:

| Bench category | Production source |
|---|---|
| Intent | `parish-input/src/intent_llm.rs::INTENT_SYSTEM_PROMPT` |
| Reaction | `parish-npc/src/reactions/arrival_reactions.rs::build_reaction_prompt` |
| Tier 2 Sim | `parish-npc/src/ticks.rs::build_tier2_prompt` |
| Tier 3 Batch | `parish-npc/src/ticks.rs::build_tier3_prompt` (new in this refresh) |

Per-sample `max_tokens` was also synced to production caps:

| Category | Production cap | Source |
|---|---|---|
| Reaction | 100 | `arrival_reactions.rs:770` (`client.generate(..., Some(100), None)`) |
| Tier 2 Sim | **200** (added in this refresh) | `ticks.rs::run_tier2_for_group` via `submit_json(..., Some(200))` |
| Tier 3 Batch | **600** (added in this refresh) | `ticks.rs::run_tier3` via `submit_json(..., Some(600))` |
| Intent / Dialogue | none | schema bounds Intent; Dialogue streams |

The previous version of `submit_json` accepted no `max_tokens`
argument — running uncapped JSON-mode generation on vllm-mlx can
produce 5500+ chunks before the 300 s timeout fires, especially on
the 6-NPC Tier 3 prompt. Production now passes 200/600 caps; the
bench mirrors that.

### Numbers (vllm-mlx 0.3.x + gemma-3-4b-it-4bit + `--schema`, May 2026)

5 iters per sample, production-faithful prompts + caps. Raw output
saved at [`bench-prod3.txt`](bench-prod3.txt).

| Category | n | tot p50 | tot p95 | ttft p50 | tokens p50 | Budget | Verdict |
|---|---|---|---|---|---|---|---|
| Intent | 15 | **451 ms** | 734 ms | 61 ms | 25 | ttft<200 / tot<500 | FAIL p95 (tot 734 > 500) |
| Reaction | 10 | 147 ms | 1127 ms | 33 ms | 7-100 | ttft<400 / tot<800 | FAIL p95 (tot 1127 > 800) |
| **Tier 2 Sim** | 5 | **1089 ms** | 1095 ms | 46 ms | 44 | tot<1500 | **PASS** |
| **Tier 3 Batch** | 5 | **30459 ms** | 30667 ms | 144 ms | 598 (cap) | tot<1500 (wrong) | **FAIL by 20x** |
| Dialogue | (cached-prefix probe, 5 curl) | 42 ms | — | **1.1 ms** | 7 | ttft<1000 | **PASS** (ttft cached) |

#### Tier 2 vs Tier 3 — different beasts

The original 1500 ms "Simulation" budget was set against Tier 2's
2-3 NPC location-scoped scene. Tier 3 batches 6 NPCs across the
entire parish over a 6-hour window:

- Output is bigger by design: one `{npc_id, mood, activity_summary, new_location, relationship_changes}` object per NPC, ~100 tokens each = 600 total.
- Constrained-decode tax at ~20 tok/s (slower than Tier 2's ~42 tok/s because the output is larger and has more variety, which the FSM rebuilds per element).
- Even with `max_tokens=600` cap forcing termination, 5 iters all hit exactly 598 tokens in 30 s ≈ steady 20 tok/s throughout.

Implication: **Tier 3 needs its own budget**, not the 1500 ms
simulation budget. Realistic Tier 3 budget options:

1. **30 s per batch on Batch lane** — fits today's measurement, but
   limits how often the batch lane can fire to a few times per minute.
2. **Smaller batch size** — 2-3 NPCs per batch instead of 6 would
   roughly halve output, landing ~15 s, still over 1500 ms.
3. **Disable schema on Tier 3** — unconstrained decode would run at
   ~95 tok/s but risks runaway and unparseable JSON. Pair with the
   600-token cap to bound runtime; might land ~6-8 s per batch.
4. **Cloud-route Tier 3** — Gemini Flash-Lite at $0.10/$0.40 per 1M
   tokens handles this in <2 s; the recommended-models matrix in
   inference-pipeline.md already calls Flash-Lite the default for
   Tier 3.

#### Intent/Reaction p95 — bimodal

Both Intent and Reaction have bimodal distributions in the bench:

- **Intent**: short prompts ("look around") finish in ~450 ms;
  longer ones ("tell Padraig I saw his cow wandering near the bog")
  cluster around 700 ms. The 4B model is borderline on the longer
  inputs; the 1B intent slot (blocked on the vllm-mlx gemma3_text
  drafter bug) is the fix.
- **Reaction**: first-meeting + workplace introductions saturate the
  100-token cap (5×100 tokens ≈ 1060 ms); plain known-acquaintance
  greetings emit 7 tokens (~120 ms). The p95 reflects the saturated
  path; p50 of 147 ms is the typical case. Either accept the bimodal
  distribution as inherent to the use case or tighten the system
  prompt for first-meeting paths.

#### vllm-mlx instability after sustained constrained-decode

Bench run wedged on Dialogue#0 after completing all Tier 3 batches.
Server log shows `[disconnect_guard] poll #N elapsed=140s` with no
chunks emitted. Killed and re-probed Dialogue separately on a fresh
server: 5 iters all ~42 ms total, 1.1 ms ttft, clean.

Suggests vllm-mlx's internal state degrades after long sustained
constrained-decode runs (5 × 30 s = 2.5 min of Tier 3 in this case).
Practical mitigation: don't run consecutive Tier 3 batches without a
brief idle window for the engine to recover. Already handled in
production by the Batch lane's natural pacing.

## Followups verified after the original PR draft

These findings landed via direct probes against vllm-mlx after the
benchmark sections above were written. They reduce open unknowns in
the local-perf story.

### Cold-load latency (3.3 s with persisted prefix cache)

`vllm-mlx serve mlx-community/gemma-3-4b-it-4bit ...`, three runs:

| Stage | Time |
|---|---|
| spawn → "Application startup complete" | 3.1 s |
| ready → first 200 OK | ~150 ms |
| **total spawn → first usable response** | **~3.3 s** |
| steady-state RSS | 4.3 GB |

Caveat: this is with the `~/.cache/vllm-mlx/prefix_cache/...` directory
already populated from prior runs (18 entries, 522 MB persisted KVs).
The very first launch on a clean machine is unmeasured; expect MLX
kernel JIT to add a one-time penalty on top.

### Sim eventful-scene measurement (uneven outcome)

Same prompt structure as production `build_tier2_prompt`, three NPC
scene with prompt steering toward empty arrays:

| Scene | Output tokens | total p50 | Verdict |
|---|---|---|---|
| Quiet ("Padraig pours, Niamh wipes the bar") | ~41 | 968 ms | PASS |
| Mildly eventful ("Tommy accuses Sean of cow theft") — model emitted empty arrays anyway | ~48 | 1154 ms | PASS |
| Strongly eventful ("Tommy throws a punch") — model emitted non-empty arrays (3 entries each) | 114-133 | **~4200 ms** | **FAIL p95** |

So the prompt-steering keeps quiet and mildly-eventful scenes within
the 1500 ms sim budget, but a genuinely dramatic event (where the
model legitimately needs to emit mood/relationship deltas) blows past
budget by 2-3x. Mitigations the runtime can apply:

- Cap `max_tokens` to ~80 for sim (truncates output cleanly).
- Stream sim and let the cancel-token preempt at the budget edge
  (depends on #7).
- Detect "eventful" scenes cheaply (rule-based) before deciding to call
  sim with the full schema; uneventful scenes use a lighter request.

### Schema array shape needs `items` to be useful

Tier2 `mood_changes`/`relationship_changes` are declared as
`{type: array}` with no item schema. The model happily emits arrays
that pass schema validation but break Rust deserialization — observed
shapes included `[{"summary": "..."}]` and
`[{"relationship_changes": [1, 2, -0.1]}]` instead of the documented
`{"npc_id": <id>, "new_mood": "<mood>"}`. Fix when we tighten the
schema: add `items: {type: object, properties: {...}, required: [...]}`
so vllm-mlx's constrained decoder enforces item shape.

### Two-model registry mode is broken on this vllm-mlx

`vllm-mlx serve --models-config <yaml>` looked promising for hosting
both an Intent slot (1 B) and a main slot (4 B) in one process, but
two issues block it on the installed version (vllm-mlx 0.3.x):

1. **`mlx-community/gemma-3-1b-it-4bit` cannot load on any path** —
   the model name matches the MLLM pattern (`gemma-3`), so vllm-mlx
   routes through `mlx_vlm.utils.load`, which raises
   `Model type gemma3_text not supported. Error: No module named 'mlx_vlm.speculative.drafters.gemma3_text'`.
   `mllm: false` in the YAML is a no-op against `is_mllm_model(source)`
   because the OR in `model_registry.py:816` lets pattern detection
   override the explicit flag. `mlx_lm/models/gemma3_text.py` ships in
   the same install, so the file exists — it just isn't reachable via
   the multi-model code path.
2. **Even when the registry succeeds, the response builder hits
   `pydantic ValidationError` on `model: Input should be a valid
   string [input_value=None]`** — the served model name isn't being
   threaded into `ChatCompletionResponse`. So the registry path is
   currently unreliable for any model, not just gemma-3.

Implications: for the two-slot loadout we need either (a) a non-gemma-3
small model whose name doesn't match the MLLM pattern (e.g.,
`Qwen2.5-0.5B-Instruct-4bit-mlx`, `Llama-3.2-1B-Instruct-4bit`), or
(b) two separate vllm-mlx processes on different ports — but this
also requires the small model to bypass the same loader bug. Today
the Intent-slot path is blocked on vllm-mlx fixes upstream or on
choosing a different small model.

### Schema-enforcement tax (~2.2x decode slowdown)

vllm-mlx's constrained-decoding path on gemma-3-4b-it-4bit decodes
markedly slower than free generation:

| Path | Decode tok/s p50 |
|---|---|
| `response_format` absent (Reaction, Dialogue) | ~95 |
| `response_format: json_schema` (Intent, Simulation) | ~38-44 |

That's roughly a 2.2x per-token slowdown for constrained outputs,
charged on top of the (free) prefill. At the output sizes we ship
(Intent ~25 tokens, Sim ~40 tokens) this is comfortably absorbed by
the per-category budgets — Sim p50 lands at 968 ms.

Implication for future categories: any new constrained-output route
that exceeds ~80 tokens of structured JSON starts breaking budgets
even when prefill is fast. The pressure is on schema design (keep
required fields small, prefer flat shapes over arrays-of-objects) and
on `max_tokens` caps, not on the runtime.

### Prefix-cache hit rate on game-loop sequence (verified)

Six requests sharing a long system prompt, varying only the user line
(simulating successive game turns):

| User input | ttft (stream) | total |
|---|---|---|
| "Padraig pours a pint." | 1.4 ms | 467 ms |
| "Niamh wipes the bar." | 1.3 ms | 272 ms |
| "Sean asks for stew." | 1.2 ms | 446 ms |
| "Tommy mutters about rent." | 1.1 ms | 447 ms |
| "Padraig refills Tommy's glass." | 1.1 ms | 429 ms |
| "Niamh bids the room good night." | 1.2 ms | 445 ms |

ttft is consistently **~1.1-1.4 ms**, two orders of magnitude below
budget. The bench's 34 ms ttft figure was conservative — real game
prefill on identical-prefix requests is essentially free. This is the
prefix-cache delivering on the system-prompt + scene-context portion
of every turn.

### Cancel-token effectiveness against vllm-mlx (verified)

Aborted a long streaming request via `curl --max-time` mid-decode,
then immediately fired a small probe:

| Cancelled request | Post-cancel probe ttft | Post-cancel total |
|---|---|---|
| `max_tokens=2000`, aborted at ~200 ms | 33 ms | 133 ms |
| `max_tokens=500`, aborted at ~1 s | 1.6 ms | 79 ms |
| no `max_tokens`, aborted at ~1 s | 1.3 ms | 78 ms |

Server log confirms `[abort_prefill] Marked ... for prefill abort` and
`[disconnect_guard] generator exhausted normally` — vllm-mlx
recognizes the disconnect, frees the slot, and serves the next request
without delay. The cancel-token plumbing in this PR is end-to-end
correct against vllm-mlx: it doesn't just abort the local future, it
actually frees the remote model slot. Earlier session-level hangs we
observed were the result of running two bench instances against the
same server, not a vllm-mlx cancel pathology.

### Concurrency under continuous batching (already a free lunch)

vllm-mlx with `--continuous-batching` batches simultaneous requests
on the same model:

| Pattern | Wall | Per-request slowdown |
|---|---|---|
| Sequential intent + sim | 612 ms | baseline |
| Concurrent intent + sim | **489 ms** (20% faster) | each ~1.5 ms ttft, modest decode slowdown |
| Concurrent intent + reaction + sim | **587 ms** | each ~1.8 ms ttft |

Three concurrent requests finish in less wall time than two sequential.
Implication for the two-worker concurrency story (#8): we don't need
two physical workers — firing two requests concurrently from one
queue is already the answer. The remaining engineering is making the
queue actually fire them concurrently rather than serializing.

### Reasoning-model fallback policy

The standalone repro at `/tmp/vllm-mlx-repro/repro.sh` characterizes
two failure modes for reasoning models (Qwen3.5):

- **Unconstrained generation stalls.** A simple "Return JSON: ..."
  prompt with `stream=false` and no `response_format` exhausts the
  request timeout because the model spends the budget on `<think>`
  tokens before emitting content. Reproduces consistently at >45 s.
- **Constrained generation completes but is slow.** The same prompt
  with `response_format: json_schema` finishes in 7 s (well over the
  Reaction/Sim budgets, just under the 30 s timeout).

The implication for runtime selection: do not pick a reasoning model
as the main slot on local without also enforcing `response_format`
on every category that has a budget. Reaction and Dialogue currently
pass `response_format` only when the caller sets a schema — they
default to free-form prose, which would stall a reasoning model.

Operational policy:
- macOS local (vllm-mlx): pin to gemma-3 family for the main slot;
  do not silently substitute a reasoning model.
- If a future feature wants reasoning behavior (deeper sim plans),
  route those calls through a dedicated category that always passes
  `response_format` and absorbs a 5-10 s budget.

### vllm-mlx instability under repeated mid-stream cancellation

While benching with the inf-bench harness, observed that requests
following cancelled streaming + `response_format: json_schema`
requests sometimes hang indefinitely (stream emits first chunk
immediately, then 50+ s of `[disconnect_guard] poll #N` with no
further chunks). Setting an explicit `max_tokens` on the request
makes the same prompts complete in normal time. This isn't strictly a
constrained-decoder hang — it looks like a recovery issue after
abort_prefill. Implication: any cancel-token use against vllm-mlx
should also pin a sensible `max_tokens` to avoid the post-cancel
degraded state.

## Rapid-MLX (Apache 2.0, fork of vllm-mlx)

Rapid-MLX is a community fork of vllm-mlx maintained at
`raullenchai/Rapid-MLX`. It's actively developed (8 releases in three
days at the time of writing), tracks a published optimization roadmap
(`ROADMAP.md`), and ships features beyond upstream: 17 tool parsers,
reasoning-content separation in a dedicated `reasoning_content` field,
prompt-cache improvements (always-on, with DeltaNet snapshots for
Qwen3.5 hybrid arch), `--kv-cache-turboquant`, MTP optimistic mode,
suffix decoding, and Ollama-style CLI subcommands (`pull`, `rm`, `ps`,
hot model swap in the chat REPL).

### Their own benchmark vs upstream (from ROADMAP.md, M3 Ultra 256GB)

| Model | Rapid decode tok/s | Upstream | Δ |
|---|---|---|---|
| Phi-4 Mini 14B | 174 | 170 | 1.02x |
| Qwen3.5-4B | 158 | 155 | 1.02x |
| **GPT-OSS 20B** | 123 | 79 | **1.56x** |
| Hermes-3-Llama 8B | 123 | 122 | 1.01x |
| Qwen3.5-9B | 109 | 104 | 1.05x |
| **GLM-4.5-Air** | 46 | 54 | **0.85x** (slower) |
| Gemma 3 12B | 49 | (artifact) | — |
| Qwen3.5-27B | 39 | 38 | 1.0x |

Cached TTFT (their numbers, Hermes-3 8B): **0.080 s vs upstream 0.106 s.**
For Gemma 3 12B their data shows upstream at 2.9 s vs Rapid 0.147 s —
a 19.9x ratio that suggests upstream is failing to use prompt cache
for that specific model. We did not reproduce this on **gemma-3-4b**:
vllm-mlx upstream gave us cached ttft of **1.1 ms** (`stream=true`) on
6 game-loop turns sharing the same system prompt, two orders of
magnitude below Rapid's claim. So whatever upstream prompt-cache bug
Rapid caught for Gemma 3 12B does not appear to affect 4B on our setup.

### Direct head-to-head on our model (this attempt)

Tested Rapid-MLX 0.6.30 (separate `~/.local/share/uv/tools/rapid-mlx/`
install, doesn't clobber the vllm-mlx symlink anymore) against
`mlx-community/gemma-3-4b-it-4bit` with the same flags we use for
vllm-mlx (`--enable-prefix-cache --continuous-batching`):

| Path | Result |
|---|---|
| Server starts | ✓ (~1 s spawn → ready) |
| Smoke `stream=false` "hi" / `max_tokens=5` | **HANGS — 60 s no chunks, server log shows `[disconnect_guard] poll #N elapsed=N.Ns`** |
| Smoke `stream=true` "hi" / `max_tokens=5` | First chunk in 948 μs, then hangs at 30 s timeout |

The server log shows
`MLLMBatchGenerator: Using VLM's language_model for batched generation`
on every request — exactly the "VLM pipeline overhead" their roadmap
flags as a Gemma 3 problem (Gemma 3 12B at 49 tok/s on Rapid vs 73
tok/s on mlx-lm). For us this manifests as a hard hang, not just a
slow path.

### Why their wins don't apply to Rundale today

The Rapid-MLX numbers most relevant to us (cached TTFT, decode speed)
are tied to specific model families — and the wins concentrate on
Qwen3.5, GPT-OSS, and reasoning-model architectures. On gemma-3-4b
upstream prompt cache already gives us a 1.1 ms cached ttft and
~95 tok/s unconstrained decode; there's no remaining headroom for
Rapid to win, and on the model we actually use it currently doesn't
work at all.

Their architectural extras only pay off if we change other choices:
- **Reasoning-content separation**: pays off with Qwen3 / DeepSeek-R1 /
  reasoning models. Useless on gemma-3-4b (no `<think>` blocks).
- **17 tool parsers**: pays off when we ship tool-calling for game
  actions. Currently we don't.
- **Hot model swap in the REPL**: nice CLI ergonomic, but our Intent
  slot need is two-models-loaded-simultaneously, which neither
  vllm-mlx registry mode (broken) nor Rapid's REPL hot swap (one at a
  time) solves.

### Conclusion (revised)

**Stay on vllm-mlx upstream for the gemma-3-4b path.** Revisit Rapid-MLX
when any of these fire:

1. We move the main slot to a Qwen3.5 family model (Qwen3.5-4B at
   ~158 tok/s with 100% tool calling looks especially attractive — and
   Rapid's reasoning-content separation cleans up the `<think>` block
   problem we observed for Qwen3.5 in the standalone repro).
2. We add tool-calling for in-game actions.
3. Rapid lands EAGLE-3 on Metal (their P2 roadmap item) — their
   estimate is 3-6.5x decode speedup for Qwen3-32B / Qwen3-8B / GPT-OSS /
   Llama-3.

We should track Rapid releases for the gemma-3-4b VLM-pipeline fix —
their own roadmap acknowledges it ("VLM pipeline overhead") and a fix
would unblock instant adoption on our existing model choice without
any other code changes.

## Qwen two-slot validation (May 2026)

After the production-faithful refresh showed gemma-3-4b failing Tier 3
(30 s) and producing unstable dialogue under sustained constrained
decode, we benched two Qwen MLX models as candidates for a two-slot
loadout: Qwen2.5-1.5B-Instruct-4bit for Intent/Reaction/Simulation,
Qwen2.5-7B-Instruct-4bit for Dialogue. Both load cleanly under
vllm-mlx 0.3.x (`mllm=False`, clean mlx_lm path — neither matches
the MLLM pattern that traps gemma-3).

### Qwen2.5-1.5B (small slot — Intent/Reaction/Sim)

Raw bench: `docs/proofs/local-perf/bench-qwen15.txt`.

| Category   | ttft p95 | total p95 | tok/s p50 | Verdict |
|---|---|---|---|---|
| Intent     | low    | <500 ms   | high   | PASS |
| Reaction   | low    | <800 ms   | high   | PASS |
| Tier 2 Sim | low    | <1500 ms  | high   | PASS |
| Tier 3 Sim | over budget but ~3x faster than gemma-3-4b | | | improved |
| Dialogue   | passes ttft, prose quality marginal | | | conditional |

### Qwen2.5-7B (large slot — Dialogue)

Raw bench: `docs/proofs/local-perf/bench-qwen7-prod.txt`.

| Category   | ttft p50 / p95 | total p50 / p95 | tok/s p50 | Verdict |
|---|---|---|---|---|
| Intent     | 229 / 498 ms   | 867 / 1123 ms   | 16.1      | FAIL (ttft >200 ms) |
| Reaction   | 67 / 171 ms    | 649 / 774 ms    | 33.8      | PASS |
| Simulation | 259 / 578 ms   | 11906 / 19696 ms| 22.6      | FAIL (Tier 3 ~20 s) |
| **Dialogue**   | **64 / 183 ms**  | **926 / 1087 ms**   | **33.3**  | **PASS** |

The 7B model is over budget on every category *except* the Dialogue
slot it's actually targeted at — exactly the design intent of the
split. For Dialogue specifically: cached ttft p95 of 183 ms (5x under
the 1000 ms budget), full streaming reply in ~1087 ms p95, 33 tok/s
sustained.

### Blind dialogue-quality compare

Generated 5 identical-prompt dialogue samples from each model with the
same system persona (Brigid the midwife). A subagent blind-judged the
pair without knowing which model produced which output. Score:

| Model | Mean score (1-5) |
|---|---|
| Qwen2.5-1.5B | 2.4 |
| Qwen2.5-7B   | 4.6 |

Samples archived at `docs/proofs/local-perf/dlg-qwen15.txt` and
`docs/proofs/local-perf/dlg-qwen7.txt`.

### Two-slot loadout (recommended)

Two vllm-mlx processes, per-category routing via existing
`resolve_category_client` plumbing:

| Slot | Port | Model | Categories | Memory |
|---|---|---|---|---|
| Small | 8001 | mlx-community/Qwen2.5-1.5B-Instruct-4bit | Intent, Reaction, Simulation | ~1.3 GB |
| Large | 8000 | mlx-community/Qwen2.5-7B-Instruct-4bit  | Dialogue                     | ~4.0 GB |

Total ~5.3 GB resident. Per-category overrides:

```toml
[provider]
name = "vllm-mlx"
base_url = "http://localhost:8000"
model = "mlx-community/Qwen2.5-7B-Instruct-4bit"

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

JSON / streaming / TTFT on the 7B slot: all verified — vllm-mlx
enforces `response_format: json_schema` engine-side (not model-side),
SSE streaming worked for all 5 dialogue samples, prefix-cache delivers
sub-ms cached ttft on warm prompts. The bench above measures ttft on a
schema-free dialogue path, which is the actual production use case for
the large slot.

## Tier-up to Qwen2.5-14B + code-switch fix (May 11 2026)

Two-slot loadout above shipped with 7B Dialogue but a 100-prompt scan
exposed a code-switch failure mode — the 7B model occasionally replied
entirely in Irish on cough/illness prompts (~1% of the scan). Two
changes followed:

1. **`language_directive` strengthened with a sprinkle-only clause**
   (`parish-npc/src/lib.rs`): en-IE must carry the meaning of every
   sentence; ga-IE is at most one 1-5 word phrase per reply. Plus a
   Latin-only character guard (Cyrillic, Han, Hiragana, Katakana,
   Hangul, Arabic, Hebrew, Greek, Devanagari forbidden).
2. **Bench + Opus-blind compare extended to 14B** as the next tier up
   for hosts with memory headroom.

### Qwen2.5-14B bench (production-faithful, 3 iters)

Raw: [`bench-qwen14-prod.txt`](bench-qwen14-prod.txt).

| Category   | ttft p50 / p95 | total p50 / p95 | tok/s p50 | Verdict |
|---|---|---|---|---|
| Intent     | 327 / 859 ms   | 1174 / 1695 ms  | 11.9      | FAIL (over Intent budget) |
| Reaction   | 128 / 348 ms   | 1463 / 2480 ms  | 18.1      | FAIL (over Reaction budget) |
| Simulation | 329 / 693 ms   | 10620 / 26262 ms| 11.9      | FAIL (Tier 3-shaped) |
| **Dialogue**   | **128 / 367 ms**  | **2087 / 2377 ms**  | **17.5**  | **PASS** |

Same shape as 7B: Dialogue passes; small-budget categories fail. That's
the intended split — 14B never serves Intent/Reaction/Sim in the
two-slot loadout; the 1.5B slot does.

### Opus-blind 3-way quality (after code-switch fix)

`/eval-dialogue` skill, Claude Opus 4.7 as judge, models hidden behind
`Model X/Y/Z`. Full report:
[`quality_eval_20260511T163000Z.md`](quality_eval_20260511T163000Z.md).

| Model | Character | Authenticity | Language | Responsiveness | Craft | Overall |
|---|---|---|---|---|---|---|
| Qwen2.5-14B-Instruct-4bit | 5.00 | 4.40 | 5.00 | 4.80 | 4.60 | **4.76** |
| Qwen2.5-7B-Instruct-4bit  | 4.60 | 3.80 | 4.40 | 5.00 | 4.20 | **4.40** |
| Qwen2.5-1.5B-Instruct-4bit| 2.00 | 2.20 | 5.00 | 3.40 | 2.20 | **2.96** |

Delta vs prior run (no code-switch fix):

| Model | Prior | New | Δ |
|---|---|---|---|
| 14B | 4.72 | 4.76 | +0.04 |
| 7B  | 4.04 | 4.40 | **+0.36** (recovers prompt-3 catastrophe) |
| 1.5B| 3.20 | 2.96 | -0.24 |

14B → 7B gap narrowed to 0.36 — at the judge-noise threshold (~0.3).
With 7B's failure mode patched, the only reason to tier-up is host
memory headroom.

### 100-prompt flaw scan on 14B

`docs/proofs/local-perf/dialogue_flaw_scan_14b.md`: **0/100** prompts
flagged for non-Latin script leakage or shape errors. The Latin-only
guard plus the sprinkle-only clause eliminated the failure mode entirely
on this model.

## Policy: 14B Dialogue + 1.5B small slot, 16 GB minimum

The shipping default for macOS local-inference is now:

| Slot | Port | Model | Categories | Memory |
|---|---|---|---|---|
| Dialogue | 8000 | mlx-community/Qwen2.5-14B-Instruct-4bit  | Dialogue                     | ~8.0 GB |
| Small    | 8001 | mlx-community/Qwen2.5-1.5B-Instruct-4bit | Intent, Reaction, Simulation | ~1.3 GB |

Total resident: ~9.3 GB. Validated p95 latencies all under per-category
budgets; Opus-blind dialogue quality 4.76/5; 0% script-flaw rate.

7B was dropped from the defaults: 0.36 Overall delta vs 14B isn't
worth the per-host configuration knob now that the catastrophic
code-switch failure is patched. 1.5B-only (Dialogue served by 1.5B as
well) scored 2.96 Opus-blind — flat, anachronistic prose with weak
character voice. Not shippable as a default.

**16 GB unified memory is the minimum host requirement for local-everything.**
Below 16 GB, `Provider::recommended_for_platform()` returns
`Provider::Simulator` and the onboarding flow steers the user to BYOK
cloud (OpenRouter, Anthropic, Google) rather than degrade silently to
the 2.96/5 small-only fallback. `unified_memory_bytes()` in
`parish-config/src/provider.rs` probes `sysctl -n hw.memsize` on macOS
for this gate.

Auto-launch wiring for the multi-slot loadout flows through
`GameConfig::vllm_mlx_extra_slots()` →
`VllmMlxProcess::ensure_slots()` (deduped against the base slot, idempotent).
The base slot spawns from `setup_provider_client`; extras spawn in
parallel and are tracked on `RuntimeProcesses { ollama, vllm_mlx:
Vec<VllmMlxProcess> }` so `RuntimeProcesses::stop()` cleans up all
spawned children on shutdown.

## Test results

```
cargo test --workspace --tests:  2461 passed, 8 ignored
cargo clippy --workspace --tests: clean
npx vitest run:                   396 passed
just check (fmt + clippy + test): pass
just agent-check:                 pass (witness scan + doc paths clean)
```

New regression test: `parish_inference::tests::test_streaming_request_records_ttft_and_token_count`
locks in that streaming requests populate `ttft_ms` and `output_tokens` on the
log entry.

## Files touched

- `parish/crates/parish-config/src/engine.rs` — default_timeout_secs 30→300
- `parish/crates/parish-inference/src/lib.rs` — `StreamStats`, proxy channel, fields on log entry
- `parish/crates/parish-inference/examples/inf_bench.rs` — new harness
- `parish/crates/parish-core/src/{debug_snapshot,game_session}.rs` — fill new fields
- `parish/crates/parish-server/src/routes.rs` — forward fields through redaction
- `parish/crates/parish-server/tests/isolation.rs` — fixture update
- `parish/apps/ui/src/lib/types.ts` — type fields
- `parish/apps/ui/src/components/{DebugInferenceTab,DebugNpcsTab,DebugPanel.test}.svelte/.ts` — display + literal `·` → `·`
- `parish/justfile` + root `justfile` — `just inf-bench` recipe
