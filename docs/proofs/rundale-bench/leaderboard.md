# rundale-bench v1 leaderboard

Evidence type: gameplay transcript

Append-only ranking of evaluated targets. One row per `(target, slice, split)` triplet. Rows added by running:

```sh
python3 parish/testing/rundale-bench/rundale_bench.py \
    --target '<model>@<base_url>#env:<KEY_VAR>' \
    --suite v1 --slice <name|all> --split <dev|holdout>
```

…then appending one row per slice + a `metric` cell appropriate to that slice (label-match rate for intent; mean overall for dialogue; mean score for reaction; schema-valid rate × mean plausibility for tier2/tier3).

## Submission rules

- **Holdout scores are the leaderboard.** Dev rows are accepted for reproducibility cross-checks but flagged as `split=dev` and ignored by ranking.
- **Re-runs replace earlier rows for the same `(target, slice, split, suite-version)` tuple.** Keep one row per tuple; cite the latest harness SHA.
- **Cost in USD must come from `CostTracker`** (the orchestrator emits it). No manual estimates.
- **`harness_sha` is `git rev-parse HEAD`** at the time of the run. If the manifest or grader changed since, re-run before quoting the row in any decision.

## Phase 6 seed rows

| Date (UTC)         | Target                                              | Slice    | Split | Records | Metric                                            | $/run  | Judge                                | Harness SHA |
|--------------------|-----------------------------------------------------|----------|-------|---------|----------------------------------------------------|--------|--------------------------------------|--------------|
| 2026-05-13 18:32   | openai/gpt-oss-120b:free @ openrouter.ai           | intent   | dev   | 30 (pre-split) | label_match=0.700, mean_score=0.676           | $0.00  | n/a (deterministic)                  | 9dab39ee… |
| 2026-05-13 20:47   | openai/gpt-oss-120b:free @ openrouter.ai           | dialogue | dev   | 5       | overall=4.28 (char 4.0 / auth 4.0 / lang 5.0 / resp 4.4 / craft 4.2) | $0.012 | claude-sonnet-4-6                    | (pre-judge-swap) |
| 2026-05-13 21:01   | openai/gpt-oss-120b:free @ openrouter.ai           | dialogue | dev   | 5       | overall=4.82 (char 4.8 / auth 5.0 / lang 5.0 / resp 4.8 / craft 4.6) | ~$0.0003 | qwen/qwen3-235b-a22b-2507 (OpenRouter) | post-judge-swap |

Pre-rundale-bench-v1.0, only dev rows exist (holdout scoring CI lands in Phase 7). The first row was produced before the holdout split was applied (`9dab39ee` ships the un-split slices); future re-runs against the same target will operate on the 25-record dev slice and the 5-record holdout slice respectively.

### Judge calibration note

The two dialogue rows above scored the **same five replies from the same target** but with different judges:

- `claude-sonnet-4-6` → overall **4.28**
- `qwen/qwen3-235b-a22b-2507` → overall **4.82**

The 0.54 delta is well above the ±0.3 noise floor stated in the plan and demonstrates that **judge model is a benchmark dimension**, not just a configuration knob. Until reproducibility is established across at least two independently-run sweeps with the same judge pin, leaderboard rows are not directly comparable across the `Judge` column. Production rankings should fix one judge per slice-version and re-score historical targets when the judge pin rotates.

The judge swap from Sonnet 4.6 → Qwen3-235B was made to keep judge cost negligible (~\$0.0003/call vs ~\$0.002/call) and to consolidate API key surface on OpenRouter. Qwen3-235B is the current pinned default in `judge_v1.json`; the Sonnet snapshot is retained in the leaderboard as a calibration row, not as a benchmark contender.

## Reading the leaderboard

A row at the top of its slice means: best measured `metric` for the largest representative N. Beware:

- **Intent** is the most reliable column — deterministic grading, no judge noise.
- **Dialogue / Reaction / Sim** scores come from a pinned LLM judge (`judge_v1` / `judge_reaction_v1` / `judge_sim_v1`); interpret deltas <0.3 as noise.
- **Cost** is *per-run*, not per-token. A cheap model with 10× the calls can show similar $.
- **Free-tier rows** (e.g. OpenRouter `:free`) show $0 but carry hidden rate-limit cost and may degrade upstream without notice. Treat them as benchmarks of an open-weight model exposed via that pipeline, not as a production target.

## Eligible targets (Phase 6 backlog)

Targets to seed once API keys are available — these are the Parish `Provider::preset_models()` picks that the benchmark is meant to defend:

- `claude-opus-4-7@anthropic.com#env:ANTHROPIC_API_KEY` — dialogue tier
- `claude-sonnet-4-6@anthropic.com#env:ANTHROPIC_API_KEY` — sim tier
- `claude-haiku-4-5@anthropic.com#env:ANTHROPIC_API_KEY` — intent tier
- `gpt-5.5@openai.com#env:PARISH_OPENAI_API_KEY`, `gpt-5.4-mini`, `gpt-5.4-nano`
- `gemini-2.5-pro@googleapis.com#env:PARISH_GOOGLE_API_KEY`, `gemini-2.5-flash`
- `openai/gpt-oss-120b:free@openrouter.ai#env:OPENROUTER_API_KEY` (this row + holdout variant)
- `llama-3.3-70b-versatile@groq.com#env:PARISH_GROQ_API_KEY`
- `grok-4.20-non-reasoning@x.ai#env:PARISH_XAI_API_KEY`
- `mlx-community/Qwen2.5-7B-Instruct-4bit@localhost:8000` — local baseline

Each lands as one PR per `(target, full --slice all sweep)`. Holdout score from each is what changes `parish-config::presets::preset_models()`.
