# rundale-bench — agent scope

Frozen, reproducible benchmark for in-character 1820 Irish gameplay inference. Drives model + provider choices in `parish-config::presets::preset_models()`. See root [`AGENTS.md`](../AGENTS.md) for non-negotiable rules. The `/rundale-bench` agent skill at [`.agents/skills/rundale-bench/SKILL.md`](../.agents/skills/rundale-bench/SKILL.md) drives this tooling from Claude Code.

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

# Then drain the judge queue and finalise:
#   /rundale-bench drain-queue
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

- **`v1-dev` — not frozen.** Dataset is intentionally undersized (155 prompts vs 1100 target). Freeze (`frozen=true` + `git tag rundale-bench-v1.0`) deferred until corpus growth and 3+ leaderboard targets on holdout.
- **SHA-256 verified loader.** Every `load_slice()` call verifies the file's SHA-256 against `MANIFEST.json`. Drift surfaces as `RuntimeError` rather than silent score change.
- **Slice record schema.** Each `*.jsonl` line: `{id, tier, persona, prompt, schema?, gold?}`. `dialogue` slice has neither `schema` nor `gold` — LLM-judge-only.
- **Two judge paths.** Default `judge_sonnet_v1` queues bundles for Sonnet subagent scoring; `judge_v1` uses the HTTP judge directly. Judge JSON files in `v1/` carry `rubric_sha256` for integrity verification.
- **Judge system prompt lives in `v1/judge_sonnet_v1.system.md`** — referenced by the bundle's `system_prompt_file` field, loaded by the `/rundale-bench-judge` subagent skill.
- **Local MLX runner** (`local_runner.py`) requires `mlx_lm` from a dedicated venv. Set `MLX_PY` env var (defaults to sibling `.venv-mlx/bin/python3`). The runner enforces a 4 GB headroom check and skips candidates whose `peak_ram_gb_est` exceeds available unified memory.
- **Output artifact naming.** `rundale-bench/artifacts/run_<target>_<slice>_<UTC>.json` per-slice; `rundale-bench/artifacts/local_<UTC>.json` for sweep aggregates. Local MLX appends a row to `artifacts/local_leaderboard.md`.
- **No modern terms.** Bench prompts stay in-character for 1820 rural Ireland — no anachronisms, no modern vocabulary.
- **English + optional Irish (ga-IE) only.** Non-Latin scripts (Cyrillic, Han, Hangul) are grounds for a score floor. The Gaeilge fluency slice explicitly tests translation, idiom, grammar, comprehension, and English-leakage resistance.
- **Models catalog** at `v1/models.toml` binds model IDs to providers with prices. Logical IDs resolved from this file in bench mode. Inferring `env:VAR` name; verify it is exported before hitting HTTP.
- **Scalability.** On a 32 GB Mac, judge + 2 local targets is comfortable; 3 pushes memory. Cloud targets use no local RAM.

## Layout

`rundale_bench.py` single-entry orchestrator, `grade.py` graders (intent, schema, dialogue, reaction, simulation, gaeilge), `catalog.py` candidate-model catalog loader, `judge_bundle.py` judge bundle dispatch, `score_multiaxis.py` multi-axis 0-10 scoring, `bench_perf.py` performance probes (ttft, tok/s, JSON compliance), `rubric_lab.py` offline rubric scoring, `local_runner.py` MLX sweep, `build_leaderboard_page.py` static HTML, `build_manifest.py` + `split_holdout.py` dataset tooling, `cache_dialogue_replies.py` + `cache.py` caching, `candidates_local_mlx.toml` local MLX fleet, `summarize_local.py` local-run summary. `v1/` holds the benchmark dataset (6 slices, judge configs, `MANIFEST.json`, `models.toml`). `artifacts/` holds generated run/perf JSON and leaderboard pages. `bench-site/` holds the static leaderboard site. `tests/` holds additional test data.
