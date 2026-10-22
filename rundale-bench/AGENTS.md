# rundale-bench — agent scope

This directory contains legacy pre-3.0 benchmark tooling and results. Current
Benchmark 3.0 lives in the public
[dmooney/rundale-bench repository](https://github.com/dmooney/rundale-bench);
the root [`/rundale-bench` skill](../.agents/skills/rundale-bench/SKILL.md)
routes current requests there. Use the commands and guidance below only when a
user explicitly requests a pre-3.0 historical run or reproduction. See root
[`AGENTS.md`](../AGENTS.md) for non-negotiable engineering rules.

## Scoped commands

```sh
# Bench one target on one slice (dev split):
python3 rundale-bench/rundale_bench.py \
    --target '<spec>' --suite v1 --slice intent --split dev --limit 30

# Full sweep — every slice, dev split:
python3 rundale-bench/rundale_bench.py \
    --target '<spec>' --suite v1 --slice all --split dev

# Holdout (gates leaderboard submission):
python3 rundale-bench/rundale_bench.py \
    --target '<spec>' --suite v1 --slice all --split holdout

# Benchmark every slice + perf in one go (sonnet subagent judge, queued):
just -f rundale-bench/justfile bench-it '<target-spec>'

# Historical pre-3.0 queue-drain instructions are preserved at:
#   https://github.com/dmooney/Rundale/blob/3bf68689ea7f6e6717c8aec2f3a6869231742cf5/.agents/skills/rundale-bench/SKILL.md
# That archived `/rundale-bench drain-queue` mode is not available in the
# current router; do not use it for Benchmark 3.0.
# Historical finalisation after an explicitly requested legacy run:
#   python3 rundale-bench/rundale_bench.py ingest --finalize

# Rebuild manifest after dataset edit:
python3 rundale-bench/build_manifest.py v1

# Regenerate static leaderboard:
python3 rundale-bench/build_leaderboard_page.py

# Perf probe:
python3 rundale-bench/bench_perf.py --target '<spec>' --prompts 10

# Local MLX sweep (requires MLX_PY env var for venv python):
just -f rundale-bench/justfile local slot=tiny slice=intent limit=25

# Unit tests (no API calls):
python3 rundale-bench/test_grade.py
```

## Local gotchas

- **`v1-dev` is not frozen.** 309 records (270 dev + 39 holdout, per `v1/MANIFEST.json`) vs 1100 target. Freeze (`frozen=true` + `git tag rundale-bench-v1.0`) deferred until corpus growth and 3+ leaderboard targets on holdout. The count is the sum of `records` across `MANIFEST.json` slices — do not hardcode it elsewhere.
- **SHA-256 verified loader.** `load_slice()` verifies each file against `MANIFEST.json`; drift surfaces as `RuntimeError`, not a silent score change.
- **Slice record schema:** `{id, tier, persona, prompt, schema?, gold?}` per `*.jsonl` line. `dialogue` slice has neither `schema` nor `gold` — LLM-judge-only.
- **One judge path.** `judge_sonnet_v1` queues bundles for Sonnet subagent scoring; the HTTP judges are disabled (see the Sonnet-only entry below). Judge JSON files in `v1/` carry `rubric_sha256`.
- **Judge system prompt:** `v1/judge_sonnet_v1.system.md` — referenced by the bundle's `system_prompt_file` field, loaded by the `/rundale-bench-judge` subagent skill.
- **Local MLX runner** (`local_runner.py`) requires `mlx_lm` in a dedicated venv. Set `MLX_PY` (defaults to `.venv-mlx/bin/python3`). Enforces a 4 GB headroom check; skips candidates whose `peak_ram_gb_est` exceeds available unified memory.
- **Output artifacts:** `rundale-bench/artifacts/run_<target>_<slice>_<UTC>.json` per-slice; `rundale-bench/artifacts/local_<UTC>.json` for sweep aggregates. Local MLX appends a row to `artifacts/local_leaderboard.md`.
- **No modern terms.** Prompts stay in-character for 1820 rural Ireland — no anachronisms or modern vocabulary.
- **English + optional Irish (ga-IE) only.** Non-Latin scripts (Cyrillic, Han, Hangul) are grounds for a score floor. The Gaeilge fluency slice tests translation, idiom, grammar, comprehension, and English-leakage resistance.
- **Models catalog** at `v1/models.toml` binds logical model IDs to providers with prices. Verify `env:VAR` is exported before hitting HTTP.
- **Memory:** Judge + 2 local targets is comfortable on 32 GB; 3 pushes it. Cloud targets use no local RAM.
- **mlx-lm 4-bit RAM footprint is roughly `params_b × 0.55 GB` peak under inference.** 70B+ 4-bit and 70B 8-bit OOM a 48 GB M5 Pro mid-generation (system reboot required). `magnum-v4-72b-4bit` and `Midnight-Miqu-70B-v1.5-MLX-8Bit` are marked `peak_ram_gb_est >= 52` in `rundale-bench/candidates_local_mlx.toml` so `local_runner.py`'s headroom check skips them. Don't lower without measuring on a bigger box.
- **`local_runner.py`'s "ready" signal is `/v1/models` returning 200, not actual model load.** Big-Tiger-Gemma-27B passed the readiness probe ~10 s before weights finished mmapping, so the first POSTs got HTTP 404 and the sweep recorded `overall=0.00` despite the model being viable later. On re-run it produced only `<pad>` tokens anyway — Gemma 4-bit quant is broken for in-character prose (same failure mode as `gemma-4-e4b-it-4bit`).
- **rundale-bench judge is Sonnet-subagent only — enforced at `load_judge`.** `local_runner.py --judge` historically defaulted to `judge_v1` = Qwen3-235B-via-OpenRouter, inflating round-4 dialogue scores by ~1.5 pts (OLMo-2-7B 4.78 under Qwen vs Sonnet's 3.28 on identical replies). Hard-locked 2026-05-28: `rundale_bench.load_judge` refuses any config whose `judge_via != "claude-code-subagent"` OR `model != "claude-sonnet-4-6"` OR `api_key_env != null`. HTTP-API judge files (`judge_v1.json`, `judge_pairwise_v1.json`) renamed `.disabled-2026-05-28-sonnet-only` so even the right id can't load them. If you add a new slice judge, ship those three invariants from day one.

## Layout

**Key files:** `rundale_bench.py` (orchestrator), `grade.py` (graders: intent, schema, dialogue, reaction, simulation, gaeilge), `catalog.py` (model catalog), `judge_bundle.py` (bundle dispatch), `score_multiaxis.py` (0-10 scoring), `bench_perf.py` (ttft/tok/s/JSON compliance), `rubric_lab.py` (offline rubric), `local_runner.py` (MLX sweep), `build_leaderboard_page.py` (static HTML), `build_manifest.py` + `split_holdout.py` (dataset tooling), `cache_dialogue_replies.py` + `cache.py` (caching), `candidates_local_mlx.toml` (MLX fleet), `summarize_local.py` (local-run summary). `v1/` holds dataset (6 slices, judge configs, `MANIFEST.json`, `models.toml`). `artifacts/` holds run/perf JSON and leaderboard pages. `tests/` holds additional test data. The published leaderboard is now the v2 site under `promptfoo/bench-site/`; the v1 `bench-site/` + `build_site_data.py` were retired in #1284.
