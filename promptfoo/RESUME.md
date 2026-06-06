# Rundale leaderboard sweep — resume instructions

The benchmark toolchain is **built, tested, and committed-ready**. The sweep paused on an
external blocker: **OpenRouter credit exhausted** (`$20.34 / $20.00` → HTTP 402 on every call).
Both the candidate catalog spine (244 of 265 viable candidates) and the pinned judge
(`openai/gpt-5.2`) run on OpenRouter, so the sweep cannot continue until it's topped up.

## To resume (once OpenRouter has credit)

```sh
cd <repo root>
set -a; source .env; set +a
export RB_JUDGE_MODEL=openai/gpt-5.2 \
       RB_JUDGE_BASE_URL=https://openrouter.ai/api/v1 \
       RB_JUDGE_API_KEY_ENV=OPENROUTER_API_KEY \
       RB_MAX_RETRIES=2 PYTHONUNBUFFERED=1

# (optional) refresh the candidate catalog from live provider model lists
just -f promptfoo/justfile enumerate            # → promptfoo/catalog/*

# Phase B — broad screen, judge-light, budget-guarded. Run per tier or all paid:
python3 promptfoo/scripts/funnel.py screen --tier budget,mid --cap 20 --limit 4 --keep 8 --concurrency 6 --yes
python3 promptfoo/scripts/funnel.py screen --tier premium    --cap 20 --limit 4 --keep 8 --concurrency 6 --yes
#   (drop --yes to print the pre-flight $ estimate without spending)

# Phase C — medium on survivors (all 7 slices, full judging)
python3 promptfoo/scripts/funnel.py medium --from-survivors --cap 30 --limit 15 --keep 3 --concurrency 6 --yes

# Phase D — full datasets + perf on the short list → leaderboard of record
python3 promptfoo/scripts/funnel.py full --from-survivors --cap 60 --concurrency 6 --yes

cat promptfoo/leaderboard/leaderboard.md
```

The funnel prints a pre-flight cost estimate and **hard-aborts** any phase that would breach
`--cap`.

### Seamless resume (just re-run the same command)

Each phase keeps a JSON checkpoint at `promptfoo/leaderboard/funnel_state.json` keyed by
(phase, tier, limit, judge, dataset merkle, slices). **Re-running the identical command auto-resumes**:
every completed (candidate, slice) is reused with zero new model/judge calls, and only missing or
**errored/402-failed** slices are re-run. So after an OpenRouter top-up you literally just re-run the
same `funnel.py screen …` line and it finishes the leftover work — the pre-flight estimate will show
`(N already complete, M to run)` and price only the remaining `M`. The checkpoint is written
atomically after every candidate, so a crash/kill/402 loses nothing.

- `--fresh` ignores the checkpoint and restarts the phase.
- Changing any keyed field (e.g. `--limit`, the judge, the datasets) auto-starts fresh (old state is
  archived to `funnel_state.json.bak`), because cached scores wouldn't be comparable.

## State at pause

- **Catalog**: 265 viable / 284 excluded (`promptfoo/catalog/{candidates,excluded}.jsonl`, `candidates.md`).
- **Datasets**: runtime-faithful, captured from a real engine run (`v2/datasets/*.jsonl`, manifest pinned).
- **Judge**: pinned `openai/gpt-5.2` (user choice; qwen3-235b rejected as too lenient; Sonnet is the
  config default). gpt-5.2 requires OpenRouter funding.
- **Free tier**: screened — **13 of 18 free models are 429-rate-limited** (unusable); 5 ran
  (laguna-m.1, gpt-oss-120b/20b, laguna-xs.2). Free models mostly have paid twins in budget/mid.
- **Budget+mid screen**: ran but **discarded** — the gpt-5.2 judge 402'd partway, so 0/732 dialogue
  items were judged. Re-run after funding.
- **Spend this session**: OpenRouter ~$1.70 (tipped it over the $20 cap); Anthropic ~$0.30 (Phase A
  Sonnet proof). Candidate spend trivial.

## If you'd rather not fund OpenRouter

Pivot to a direct-provider-only benchmark (no OpenRouter): re-enumerate with only the direct keys
(Anthropic Claude, Google Gemini, DeepSeek, NVIDIA NIM) and set the judge to an Anthropic-direct
model. Candidate coverage shrinks to those families, but it needs no OpenRouter credit.
