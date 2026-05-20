# rundale-bench v1-dev — ELO pairwise mode

Evidence type: gameplay transcript

The absolute 5-axis rubric saturated near the ceiling (gpt-oss-120b:free scored 4.82/5 on its own — no headroom). Pairwise ELO replaces it as the authoritative dialogue ranking method.

## Changes

- `rundale-bench/v1/judge_pairwise_v1.json` — new pinned judge config. Same model as `judge_v1` (qwen3-235b on OpenRouter) but pairwise rubric; `rubric_sha256 = b5664f96…dc7c0` independent from `judge_v1.rubric_sha256`.
- `grade.py::grade_pairwise(reply_a, reply_b, prompt, judge, invoke)` — judge picks A | B | tie + one-sentence reason. Non-Latin script in one reply auto-disqualifies that reply (judge can't be trusted to enforce this consistently).
- `rundale_bench.py --mode elo` — new top-level mode. Takes repeated `--target` flags (≥2 required), runs every candidate over the dialogue slice once, then schedules a pairwise match per prompt per (a,b) pair with randomized position. ELO accumulates with K=32 → K=16 after 50 matches per candidate. Bootstrap 5/95 CI over 500 resamples.
- `test_grade.py` — 5 new tests for `grade_pairwise` (winner, tie, invalid winner, non-Latin auto-DQ, rubric tamper). 27/27 pass.

## Smoke run

```sh
python3 rundale-bench/rundale_bench.py \
    --mode elo \
    --target 'openai/gpt-oss-120b:free@https://openrouter.ai/api/v1#env:OPENROUTER_API_KEY' \
    --target 'qwen/qwen3-235b-a22b-2507@https://openrouter.ai/api/v1#env:OPENROUTER_API_KEY' \
    --target 'mistralai/mistral-small-24b-instruct-2501@https://openrouter.ai/api/v1#env:OPENROUTER_API_KEY' \
    --suite v1 --limit 10
```

30 candidate generations (3 candidates × 10 prompts) + 30 pairwise judgements = 60 OpenRouter calls. 346 s wall. ~\$0.0013 total.

### Standings

| Rank | Target                                       | ELO    | 5/95 CI           | Matches |
|------|----------------------------------------------|--------|-------------------|---------|
| 1    | qwen/qwen3-235b-a22b-2507                    | 1646.2 | 1598.6 – 1694.0   | 20      |
| 2    | openai/gpt-oss-120b:free                     | 1497.1 | 1437.0 – 1561.2   | 20      |
| 3    | mistralai/mistral-small-24b-instruct-2501    | 1356.6 | 1306.3 – 1399.8   | 20      |

290-point spread across three candidates with non-overlapping 5/95 CIs between Qwen and Mistral. The absolute rubric mode crushed this same set into the 4.5–4.9 / 5 band.

Full run JSON: `elo_20260513T221506Z.json` — includes per-match reason strings (judge's one-line rationale) for spot-auditing biases.

## Caveats

- **Same-family bias.** Qwen3-235B is the judge AND the top-ranked target. Sanity check pending with a deepseek/* or anthropic/* judge before this row drives a preset decision.
- **N=10 prompts is below the comfort floor.** Plan calls for 25-prompt minimum; this is a smoke. Bootstrap CI tightens with more matches.
- **i.i.d. match resampling for CI understates uncertainty** when matches are correlated by prompt. Prompt-level bootstrap is a future refinement.
- **ELO assumes transitive preferences**, which judges roughly satisfy but don't guarantee. Non-transitivity (A>B>C>A) shows up as oscillating ratings; not observed in this smoke but worth watching with larger N.

## What this unblocks

Preset-selection decisions can now be defended with measured pairwise dominance rather than fuzzy absolute scores. The next concrete step is a 6-target × 25-prompt sweep against the candidate list documented in `leaderboard.md::Eligible targets (Phase 6 backlog)`, costed at < $0.10 with the qwen judge.
