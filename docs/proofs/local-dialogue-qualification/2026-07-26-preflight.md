# Local dialogue qualification preflight — 2026-07-26

This is a rejection record, not a promotion receipt. No fully local dialogue
profile was added to the production qualification registry.

## Frozen evaluation input

- Dataset manifest:
  `feb43d4bf80dae70b1cda3f506f58db7a26602a7a24d2a42f3453cf3726ec912`
- Capture source: exact requests emitted by the shared game runtime after the
  anti-rumour grounding prompt was added.
- Holdout sizes: 126 dialogue, 30 multiturn, 4 reaction, 31 Tier 2, and 1
  Tier 3 record.
- Promotion policy:
  `promptfoo/config/dialogue_promotion.json`

## Current-manifest local preflight

| Candidate | Request profile | Live turns | Contract-valid | Guard interventions | Result |
| --- | --- | ---: | ---: | ---: | --- |
| `mlx-community/Qwen3.5-9B-MLX-4bit` on MLX-LM | temperature 0.3, frequency penalty 0.5, JSON mode, thinking disabled, 768 max tokens | 12 | 12/12 full JSON | 12/12 `mood_register_guard` | Rejected before holdout promotion |

The live run used the shared Parish server path and canonical NPC-turn parser
and guard telemetry. Although every response satisfied the JSON contract, raw
responses still asserted unsupported first-hand knowledge about named parish
people. That is a zero-tolerance fabrication signal under the promotion
policy. The 100-record judged holdout and 500-turn soak were therefore not run:
the staged funnel stops candidates at the first decisive failure.

The 12/12 mood intervention rate is independently over the 10% promotion
ceiling. It is not treated as a quality score; it is evidence that the model
still depends too heavily on deterministic rewriting in this scenario.

## Earlier exploratory runs

These runs used the preceding prompt manifest and are retained only as
experiment-selection evidence. They cannot qualify the current prompt.

| Candidate | Observed result | Decision |
| --- | --- | --- |
| Qwen 3.5 9B MLX 4-bit, temperature 0.7 | 20/20 structurally valid; fabricated people, places, and events in displayed/raw dialogue | Reject |
| Qwen 3.5 9B MLX 4-bit, temperature 0.3 | 12/12 structurally valid; invented named-person sightings and physical details | Reject |
| Qwen 2.5 14B Instruct MLX 4-bit, temperature 0.3, 256 max tokens | 12/12 structurally valid; verbosity guard on 11/12 turns and one grounding intervention; 4.8–19.2 second completion times | Reject |

An isolated Qwen 3.5 9B performance sweep on the same Apple M5 Pro host
measured cold p95 TTFT/completion of 4.909/9.595 seconds, warm p95
TTFT/completion of 0.197/4.860 seconds, median throughput of 31.64 tokens/s,
0/18 request errors, and roughly 12 GB peak model memory. Those provisional
performance numbers clear the configured latency/resource thresholds, but
performance cannot compensate for a dialogue hard failure and does not confer
qualification.

## Product result

- The exact-pair qualification registry remains empty.
- Built-in local presets are labeled experimental.
- Setup recommends BYOK dialogue when a runnable local backend has no passing
  qualification receipt.
- The qualification drift check rejects any future registry entry without a
  passing, current-manifest, content-addressed promotion receipt.
- Cloud judge scoring was not invoked because it would transmit frozen
  fictional prompts and generated dialogue to an external provider without
  explicit user consent. Decisively rejected candidates do not need paid
  judging to remain rejected.
