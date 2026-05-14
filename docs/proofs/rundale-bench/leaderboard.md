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

## ELO standings — dialogue slice (`--mode elo`)

The absolute 5-axis rubric saturates near the ceiling (gpt-oss-120b scored 4.82/5 on its own — no headroom for stronger models to differentiate). Pairwise ELO replaces it for ranking decisions. Each match: judge picks A / B / tie between two candidate replies on the same prompt, with A/B position randomized per match to absorb first-position bias. ELO starts at 1500, K=32 initially (drops to 16 after 50+ matches/candidate), bootstrapped CI via 500 resamples.

| Date (UTC)       | Targets   | Prompts | Matches | Judge                          | Cost    | Run file |
|------------------|-----------|---------|---------|--------------------------------|---------|---|
| 2026-05-13 22:15 | 3 (smoke) | 10      | 30      | judge_pairwise_v1 (qwen3-235b) | $0.0013 | `elo_20260513T221506Z.json` |

### Standings — 3-candidate smoke (qwen3-235b judge)

| Rank | Target                                       | ELO    | 5/95 CI           | Matches |
|------|----------------------------------------------|--------|-------------------|---------|
| 1    | qwen/qwen3-235b-a22b-2507                    | 1646.2 | 1598.6 – 1694.0   | 20      |
| 2    | openai/gpt-oss-120b:free                     | 1497.1 | 1437.0 – 1561.2   | 20      |
| 3    | mistralai/mistral-small-24b-instruct-2501    | 1356.6 | 1306.3 – 1399.8   | 20      |

~290 ELO spread across three candidates with **non-overlapping CIs between Qwen and Mistral** — discrimination the absolute rubric was crushing. GPT-oss-120b lands middle.

### Standings — 12-candidate sweep, 2026-05-14 (mistral-large-2512 judge)

Cached samples: `dialogue_samples_20260514T004513Z.json` (6 paid-cheap) + `dialogue_samples_20260514T005721Z.json` (6 mid-tier) → 12 candidates × 15 prompts. 816 pairwise matches over `judge_pairwise_v1` rubric judged by `mistralai/mistral-large-2512` ($0.50/$1.50 per M). ~22 minutes wall, < $0.10.

| Rank | Target                                          | ELO    | Matches | Notes |
|------|-------------------------------------------------|--------|---------|-------|
| 1    | qwen/qwen3-235b-a22b-2507                       | 1898.9 | 149     |       |
| 2    | anthropic/claude-haiku-4.5                      | 1768.4 | 149     | strong for cheap-tier ($1/$5) |
| 3    | google/gemma-3-27b-it                           | 1705.0 | 149     |       |
| 4    | mistralai/mistral-large-2512                    | 1682.6 | 149     | **same as judge — self-bias inflates this row** |
| 5    | moonshotai/kimi-k2.5                            | 1622.8 | 11      | **only 11 matches — reasoning-model output empty for 14/15 prompts; row unreliable** |
| 6    | x-ai/grok-3-mini                                | 1484.9 | 149     |       |
| 7    | deepseek/deepseek-v3.2                          | 1473.3 | 149     |       |
| 8    | openai/gpt-oss-120b                             | 1356.8 | 131     |       |
| 9    | google/gemini-2.5-flash                         | 1340.9 | 149     |       |
| 10   | mistralai/mistral-small-24b-instruct-2501       | 1305.2 | 149     |       |
| 11   | openai/gpt-4o-mini                              | 1242.6 | 149     |       |
| 12   | microsoft/phi-4                                 | 1118.6 | 149     | bottom by 124 points |

780-point top-to-bottom spread. Findings:

- **qwen3-235b dominant.** Top by 130 ELO over Claude Haiku 4.5 — but qwen3-235b is also the previous judge pin, so absorbed weight from the prior pairwise rubric's training of itself. Treat this row with the same suspicion as #4 (mistral-large self-bias).
- **claude-haiku-4.5 punches above its tier.** Mid-cost ($1/$5 per M) but second-place on quality — strong candidate for the Dialogue preset.
- **Cheap mistral-small at #10**, big mistral-large at #4. ~377-point gap within the same family — model size + capability matters, the family label doesn't.
- **microsoft/phi-4 dead last** by 124 ELO; not a dialogue candidate worth pursuing for 1820 Irish.

### Caveats

- **Judge self-bias.** mistral-large at #4 against 11 cross-family competitors is suspect; comparable cross-judge runs (qwen judge / claude judge) should adjust this. Plan: re-run with a non-candidate judge (e.g. cohere/command-a) and average the two ranking tables.
- **Reasoning-class models break the cache.** `moonshotai/kimi-k2.6` and `moonshotai/kimi-k2.5` both return `content: null` with all output in `reasoning` field — current `call_chat` only reads `content`, so their cached replies are empty. Same problem hit `z-ai/glm-4.7`. Until the cache supports `reasoning` fallback OR we exclude reasoning models, these candidates can't be ELO-ranked.
- **Position-bias absorbed** by per-match A/B randomization (seed 0xe10 in rubric_lab.py); same approach as Chatbot Arena.
- **N=15 prompts per candidate** is below the 25-prompt comfort floor. Larger prompt counts tighten the standings — bootstrap CI not computed in this rubric_lab run (only in the in-bench `--mode elo` path).

### Caveats for ELO rows

- **Judge is also a competitor.** Qwen3-235B is the judge AND the top-ranked target here. Same-family bias is plausible. Re-run with a deepseek/* or anthropic/* judge before quoting the top of the table in a preset decision.
- **N=10 prompts is below the comfort floor.** Plan calls for 25-prompt minimum; this is a smoke. Re-run at 25-50 prompts before treating ELO numbers as authoritative.
- **Bootstrap CI uses i.i.d. resampling of matches**, which understates uncertainty when matches are correlated by prompt. A prompt-level bootstrap would give wider, more honest bounds.

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
