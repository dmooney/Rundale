# OpenRouter dialogue cost search — 2026-07-27

## Outcome

No tested OpenRouter profile passed every deterministic production dialogue
gate. No subjective judging was run: each candidate was disqualified by
preflight or performance first.

The strongest cost/performance lead is
`moonshotai/kimi-k2-0905:nitro`. It produced 12/12 full-JSON production turns
with zero guard interventions and, over an expanded 48-measurement performance
panel, had zero errors, 689 ms median TTFT, and 722 ms median completion.
However, its warm p95 was 1,336 ms TTFT / 1,344 ms completion, above the
1,000 ms TTFT promotion limit.

## Fixed decision contract

Candidates were called through the production server dialogue path with:

- `max_tokens = 768`
- `temperature = 0.7`
- `frequency_penalty = 0.5`
- JSON mode enabled
- intent, simulation, and reaction routed to the simulator to isolate dialogue

The deterministic funnel was:

1. Twelve production-path calls: 100% valid response and at most 10% guard
   interventions.
2. Serial streaming performance: cold TTFT p95 at most 6,000 ms, warm TTFT p95
   at most 1,000 ms, median throughput at least 15 tokens/s, cold completion
   p95 at most 10,000 ms, warm completion p95 at most 5,000 ms, and error rate
   at most 0.5%.
3. Only a deterministic survivor may proceed to blind dialogue judgment.

## Search results

Projected costs use the 2026-07-27 OpenRouter catalog floor price and Rundale's
committed all-category normal-play token profile. Routed `:nitro` requests can
cost more when the fastest endpoint is not the catalog-floor endpoint.

| Profile | Projected game-hour | Preflight | Performance result | Decision |
|---|---:|---|---|---|
| `openai/gpt-oss-120b:nitro` | $0.036 | 9/12 full JSON | Not run | Reject: structural reliability |
| `openai/gpt-4.1-mini` | $0.367 | 12/12 JSON, 0 guards | warm TTFT p95 2,416 ms; completion p95 5,716 ms | Reject: latency |
| `google/gemini-2.5-flash` | $0.398 | 11/12 valid; 1 guard | Not run | Reject: structural reliability |
| `mistralai/mistral-medium-3.1` | $0.405 | 12/12 JSON; 5 guards | Not run | Reject: 41.7% guard rate |
| `nvidia/nemotron-3-ultra-550b-a55b` | $0.477 | 4/12 JSON; 3 guards | Not run | Reject: structure and guards |
| `z-ai/glm-4.5v` | $0.493 | 10/12 JSON; 4 guards | Not run | Reject: structure and guards |
| `qwen/qwen2.5-vl-72b-instruct` | $0.525 | 12/12 JSON, 0 guards | 8.1 tokens/s; warm completion p95 20,904 ms | Reject: throughput and latency |
| `moonshotai/kimi-k2-0905` | $0.560 | 12/12 JSON, 0 guards | warm TTFT p95 2,950 ms | Reject: latency |
| `moonshotai/kimi-k2-0905:nitro` | route-dependent | 12/12 JSON, 0 guards | expanded warm TTFT p95 1,336 ms | Reject: latency; best lead |
| `z-ai/glm-5.2` | $0.652 | 12/12 JSON, 0 guards | warm TTFT p95 5,896 ms | Reject: latency |
| `x-ai/grok-4.3` | $0.909 catalog floor | 12/12 JSON, 0 guards | warm completion p95 12,485 ms | Reject: completion latency |
| `x-ai/grok-4.3:nitro` | route-dependent | 12/12 JSON, 0 guards | warm TTFT p95 2,022 ms; completion p95 9,654 ms | Reject: latency |
| `openai/gpt-4o` | $2.292 | 12/12 JSON; 2 guards | Not run | Reject: 16.7% guard rate |

Raw receipts are under
[`runs/2026-07-27`](runs/2026-07-27/). The expanded performance artifacts are
the authoritative Kimi Nitro and GPT-4.1 mini measurements; the shorter Kimi
Nitro artifact is retained as an immutable earlier attempt.

## Harness findings fixed during the search

1. A valid single-chunk stream can have `total_ms == ttft_ms`, making
   post-first-token throughput unmeasurable. The report formerly counted that
   as a request error. It now keeps the row in latency/error accounting and
   conservatively omits it only from the throughput distribution.
2. OpenRouter returns the exact charged cost in the final streaming usage
   object. The harness formerly discarded it and substituted a stale static
   price table. Streaming candidates now preserve provider-observed cost.

## Runtime finding

Player dialogue in the web-server conversation path uses the base inference
queue rather than resolving the configured dialogue category client. The first
Mistral attempt therefore reached the simulator despite a dialogue override.
For valid measurements, the base provider/model was set to the candidate and
all non-dialogue categories were explicitly routed to the simulator.

This needs a separate runtime fix and a server integration test proving that a
dialogue category override reaches both the wire request and telemetry.

## Next campaign

The next run should begin only with at least $20 remaining OpenRouter credit.
OpenRouter documents extra balance checks and more aggressive cache expiry when
credit is in the single digits; the campaign began at approximately $8.81
remaining, so these measurements are valid rejections under the observed
conditions but not a clean estimate of normal funded-account tail latency.

Then:

1. Re-run Kimi K2 0905 Nitro as the incumbent with at least 100 serial warm
   measurements and provider-observed cost.
2. Add OpenRouter-native request controls to Rundale: `provider.sort`,
   `preferred_max_latency`, `max_price`, and unified `reasoning.effort`.
3. Establish the cheapest deterministic survivor using provider-constrained
   profiles, not model slug alone.
4. Binary-search price downward from that survivor.
5. Ask for explicit authorization before sending frozen fictional prompts and
   outputs to the external blind judge. Do not judge any deterministic reject.

