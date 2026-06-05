# rundale-bench v2 (promptfoo)

A [promptfoo](https://promptfoo.dev) port of `rundale-bench` for 1820-Irish NPC
inference. It reproduces every dimension of v1 — **dialogue** (5-axis),
**reaction**, **tier2-sim / tier3-sim** (schema + plausibility), **intent**
(deterministic), **gaeilge** (Irish, 5-axis), **performance** (p50/p95 latency,
TTFT, tokens/sec), and **cost / game-time** (provider price × tokens-per-game-
minute → USD/min·hr) — with one change: the judge is a **configurable HTTP API**
model instead of v1's mandatory Claude Code subagent.

The legacy `rundale-bench/` tree is untouched; v2 is self-contained here. Frozen
datasets and rubrics are **copied** into `v2/` (sha-pinned in `v2/MANIFEST.json`,
byte-identical to `rundale-bench/v1/`); the HTTP call layer (`eval_lib`) and the
deterministic graders (`grade.py`) are **imported** so candidate behaviour stays
identical to v1.

## Dimension → promptfoo construct

| Dimension             | How                                                                                           |
| --------------------- | --------------------------------------------------------------------------------------------- |
| dialogue              | `assertions/rubric_judge.py` → 5 axes as `namedScores` + `overall`                            |
| reaction              | `rubric_judge.py` → `in_character`                                                            |
| tier2-sim / tier3-sim | `assertions/schema_assert.py` (deterministic schema-valid) + `rubric_judge.py` (plausibility) |
| intent                | `assertions/intent_assert.py` (exact label + Jaccard; no judge)                               |
| gaeilge (irish)       | `rubric_judge.py` → fluency, grammar, idiom, task_fulfillment, english_leakage                |
| performance           | streaming `providers/rundale_candidate.py` records TTFT + tok/s; `scripts/report.py` rolls up |
| cost / game-time      | native `tokenUsage`+`cost`; `report.py` × `config/pricing.py` profile → USD/min·hr            |

## Run

```sh
# 1. install promptfoo (once)
cd promptfoo && npm install && cd ..

# 2. set provider/judge keys (cloud judge default = Sonnet via Anthropic OpenAI-compat)
export ANTHROPIC_API_KEY=sk-ant-...        # judge
export OPENAI_API_KEY=sk-...               # whichever candidate you pick

# 3. one target, every slice + perf + report (limit=10 for a quick pass)
just -f promptfoo/justfile bench \
  'gpt-5-mini@https://api.openai.com/v1#env:OPENAI_API_KEY' 10

# or a single slice
just -f promptfoo/justfile dialogue 'mlx-community/Qwen2.5-7B-Instruct-4bit@http://localhost:8000/v1'
just -f promptfoo/justfile report   # roll up output/*.json
```

`bench` writes `output/<slice>.json` per slice then `output/report.md` +
`report.json`. Candidate target = `$RB_TARGET` (a `model@base_url[#env:VAR]`
spec); see `config/targets.yaml` for named examples.

## Configurable judge

`config/judge.yaml` sets the judge model / base_url / api_key_env / temperature.
Default is `claude-sonnet-4-6` via Anthropic's OpenAI-compat endpoint. Override
per-run with env vars — point the judge at any OpenAI-compatible endpoint:

```sh
RB_JUDGE_MODEL='openai/gpt-oss-120b' \
RB_JUDGE_BASE_URL='https://openrouter.ai/api/v1' \
RB_JUDGE_API_KEY_ENV='OPENROUTER_API_KEY' \
  just -f promptfoo/justfile dialogue '<target>'
```

The copied rubric text is sha-pinned (`v2/rubrics/judge_*.json`) and verified on
every judge call, so a silent rubric edit fails loudly.

## Offline self-test

`python3 promptfoo/scripts/test_v2.py` exercises every Python seam (loader,
candidate request shapes, deterministic asserts, judge bundle assembly +
envelope parse, report aggregation, game-time cost) with mocked HTTP — no keys,
no network. `scripts/mock_server.py` is an OpenAI-compat test double for a full
keyless end-to-end promptfoo run.

## Layout

```
config/      judge.yaml · targets.yaml · pricing.py (COSTS + game-time profile)
v2/          datasets/*.jsonl · rubrics/*.system.md+*.json · MANIFEST.json · perf.ids.json
providers/   rundale_candidate.py   (one candidate call → output+usage+cost+ttft/tps)
assertions/  intent_assert.py · schema_assert.py · rubric_judge.py
prompts/     passthrough.py
scripts/     load_dataset.py · report.py · build_manifest.py · test_v2.py · mock_server.py
promptfooconfig.<slice>.yaml   (one per slice + perf)
rb_common.py (bridge to eval_lib + grade; per-slice request builders; judge plumbing)
```
