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

## Leaderboard of record — rank every viable model

Beyond evaluating one hand-picked target, the suite enumerates **every viable model**
across all keyed providers, scores them on **runtime-faithful** prompts, and ranks them
in a committed, append-only leaderboard.

```sh
# 1. REQ 1 — enumerate every viable candidate from provider model lists
just -f promptfoo/justfile enumerate            # → promptfoo/catalog/{candidates,excluded}.{jsonl} + candidates.md

# 2. REQ 2 — capture byte-exact runtime prompts and rebuild the datasets
just -f promptfoo/justfile capture-prompts      # drives the real engine → v2/datasets/*.jsonl (+ re-pins MANIFEST)

# 3. phased funnel (budget-guarded): screen ALL → medium survivors → full short-list
python3 promptfoo/scripts/funnel.py screen --tier free --cap 5  --limit 4   # estimate only
python3 promptfoo/scripts/funnel.py screen --tier free --cap 5  --limit 4 --yes
python3 promptfoo/scripts/funnel.py medium --from-survivors --cap 30 --limit 15 --yes
python3 promptfoo/scripts/funnel.py full   --from-survivors --cap 60 --yes

# 4. the leaderboard (also written by the funnel each phase)
cat promptfoo/leaderboard/leaderboard.md        # ranked; full history in leaderboard.jsonl
```

- **REQ 1 enumeration** (`scripts/enumerate_candidates.py`) queries OpenRouter / Anthropic /
  Google / DeepSeek / NVIDIA NIM `/models` + the opencode-go cache + local MLX, applies a
  documented viability filter (chat/instruct · text-only · context floor · JSON-capable ·
  interactive cost ceiling), de-dups by family, buckets by `$/game-hour` tier.
- **REQ 2 runtime-faithful prompts** (`scripts/capture_server.py` + `capture_prompts.sh` +
  `build_runtime_datasets.py`): the real engine is pointed at a recording stub and driven over a
  scripted tour; the **byte-exact** requests it sends (dialogue with PEOPLE YOU KNOW / WHAT'S ON
  YOUR MIND / anchors / `json_object` / `frequency_penalty`, intent, reaction, tier2/tier3 sim)
  are folded into the datasets and sent **verbatim** by the bench — no reconstruction, no engine
  change. A fresh capture is a new sample of the game's stochastic simulation,
  not an expected byte-for-byte rebuild. The committed sample is the
  reproducible baseline: its files are content-addressed by `MANIFEST.json` and
  replayed byte-exactly for every candidate. The drift guard in
  `scripts/test_v2.py` fails if the datasets lose their runtime shape.
- **REQ 3 multiturn** (`promptfooconfig.multiturn.yaml`, `v2/rubrics/judge_multiturn_v1.*`):
  a scripted multi-turn conversation per record, the candidate's own replies chained as assistant
  turns, judged on the four known failure modes (re-introduction, wrong name, premature farewell,
  persona/memory drift).
- **REQ 4 benchmark of record** (`scripts/leaderboard.py`): per-category means with **95% bootstrap
  CIs**, gameplay-token-weighted **overall**, catalog-priced **$/game-hour** and **value**, p50/p95.
  Append-only `leaderboard.jsonl` (history) + ranked `leaderboard.md` (latest per candidate). Every
  row records its `judge_model` + dataset merkle so re-runs are comparable.
- **Funnel** (`scripts/funnel.py`): runs in-process through the same `rb_common` request + judge
  code as the promptfoo provider/assertion (so scoring is identical) but fast enough for hundreds of
  candidates, emitting promptfoo-shaped `output/*.json`. A pre-flight estimate **hard-aborts** any
  phase that would breach `--cap`. `--tier`, `--from-survivors`, `--limit`, `--keep` shape each phase.

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
just -f promptfoo/justfile dialogue 'mlx-community/Qwen2.5-14B-Instruct-4bit@http://localhost:8000/v1'
just -f promptfoo/justfile report   # roll up output/*.json
```

`bench` writes `output/<slice>.json` per slice then `output/report.md` +
`report.json`. Candidate target = `$RB_TARGET` (a `model@base_url[#env:VAR]`
spec); see `config/targets.yaml` for named examples.

## Reports

Every eval emits both `output/<slice>.json` (consumed by `scripts/report.py`)
and a standalone `output/<slice>.html` web report (both gitignored).

- **Interactive web UI** — `just -f promptfoo/justfile view` launches promptfoo's
  local browser app over the most recent eval(s): each candidate × prompt as a
  grid, the judge's **per-axis scores as columns** (character, authenticity, …
  — they ride in as `namedScores`), latency, tokens and cost; click a cell for
  the full prompt/response and the judge's reason.
- **Static HTML** — open `output/<slice>.html` directly; self-contained, no server.
- **Cross-slice rollup** — `just -f promptfoo/justfile report` →
  `output/report.md` + `report.json` with the leaderboard-style per-slice means,
  the perf rollup (p50/p95, tok/s), and the cost/game-time projection
  (USD/min·hr). promptfoo's own UI is per-slice and doesn't compute these.

## Production promotion gate

Leaderboard rank is exploratory evidence, not permission to change a shipped
local preset. A model/backend/hardware combination is promotable only when
[`scripts/promotion_gate.py`](scripts/promotion_gate.py) emits a passing,
content-addressed receipt against the frozen holdout:

```sh
# Generate holdout outputs. The provider stamps RB_SPLIT into every row so the
# gate can reject development output.
just -f promptfoo/justfile bench-holdout '<target-spec>'

# With the candidate configured on a running Parish backend, collect 500 live
# turns through the canonical parser and guard path. The command is resumable.
just -f promptfoo/justfile soak '<target-spec>' artifacts/soak.json 500

# rundale-bench/local_runner.py supplies the independently sampled peak-memory
# artifact. Assemble (and hash) both measurement sources.
just -f promptfoo/justfile build-evidence \
  '<target-spec>' apple-silicon-24-32gb \
  artifacts/soak.json rundale-bench/artifacts/local_<timestamp>.json \
  artifacts/profile-evidence.json

just -f promptfoo/justfile promote \
  '<target-spec>' artifacts/profile-evidence.json
```

The policy is machine-readable in
[`config/dialogue_promotion.json`](config/dialogue_promotion.json). It requires:

- at least 100 dialogue and 30 multiturn holdout records;
- at least 95% player-ready dialogue turns, with the Wilson 95% lower bound at
  or above 90%;
- dialogue mean at least 3.8/5 and bootstrap lower bound at least 3.6;
- no fabrication, degenerate-loop, non-Latin, refusal, or empty-output signal;
- a 500-call production-parser soak with at least 99.5% complete, non-empty
  Tier-1 JSON responses (heuristic recovery remains a player-safety fallback,
  not a contract-valid success);
- guard intervention on no more than 10% of at least 500 observed turns;
- six distinct cold NPC-prefix measurements and at least ten warmed-prefix
  measurements: cold TTFT/completion p95 at most 6s/10s, warmed TTFT/completion
  p95 at most 1s/5s, median throughput at least 15 tok/s, and overall error
  rate no more than 0.5%;
- peak local-model memory no greater than 80% of the registered hardware
  profile.

Hardware classes are registered in
[`config/local_hardware_profiles.json`](config/local_hardware_profiles.json).
The promotion receipt records both the dataset merkle and a digest of the
promotion policy, so results from different corpora or thresholds cannot be
presented as comparable.

The gate deliberately fails when a required measurement or holdout slice is
missing. Preliminary leaderboard rows and development runs remain useful for
selecting experiments, but cannot qualify a production preset.

Shipped qualification claims are separately fail-closed. Exact passing
provider/model pairs live in
`parish/crates/parish-config/src/local_dialogue.rs`; setup calls every other
local profile experimental. `just -f promptfoo/justfile qualification-check`
requires a passing receipt for the current frozen manifest before a registry
entry is valid and rejects a `Recommended` label on unqualified local presets.
The registry is currently empty.

Performance runs distinguish cache state rather than averaging it away. The
first pass uses one byte-distinct system prompt per NPC and is stamped `cold`;
later repeats are stamped `warm`. The server must be freshly started for a
qualification run. This catches both first-conversation prefill cost and the
steady-state path players experience during an ongoing conversation.

`soak_dialogue.py` reads its metrics from a diagnostic event emitted by the
shared live NPC-turn path. The synchronous command response aggregates that
event under `kind_detail.dialogue_quality`; the soak does not maintain a
Python copy of the production parser or guards. Promotion re-hashes the raw
turn JSONL and local-runner artifact and recomputes every reliability, guard,
and memory summary before issuing a receipt.

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

## Known limitations

- **`run spend` excludes the API judge's cost.** The judge call happens inside
  the assertion process, which can't add to a promptfoo result row's `cost`, so
  the reported per-run spend sums only candidate-provider cost. The headline
  cost metric — `gameplay_cost_usd_per_minute/hour` — is a price×token-profile
  projection and is unaffected. (Attributing judge spend would need an
  assertion→`report.py` side-channel.)
- **Perf `usd_per_mtok_observed` can read 0 for streaming-only providers** that
  emit token usage only when `stream_options:{include_usage:true}` is set, which
  `eval_lib.call_chat_streaming` does not request. Only this observed-rate
  diagnostic is affected; the game-time cost projection uses the static profile.

## Layout

```text
config/      judge.yaml · targets.yaml · pricing.py (COSTS + game-time profile)
v2/          datasets/*.jsonl · rubrics/*.system.md+*.json · MANIFEST.json · perf.ids.json
providers/   rundale_candidate.py   (one candidate call → output+usage+cost+ttft/tps)
assertions/  intent_assert.py · schema_assert.py · rubric_judge.py
prompts/     passthrough.py
scripts/     load_dataset.py · report.py · pin_manifest.py · test_v2.py · mock_server.py
promptfooconfig.<slice>.yaml   (one per slice + perf)
rb_common.py (bridge to eval_lib + grade; per-slice request builders; judge plumbing)
```
