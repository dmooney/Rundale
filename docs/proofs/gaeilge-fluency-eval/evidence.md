Evidence type: live gameplay transcript

Note: this is a `rundale-bench` eval slice, not a game fixture. The live
process exercised here is the Python benchmark/test CLI; no game backend is
involved.

# Gaeilge Fluency Eval Evidence

## Commands run

```sh
python3 parish/testing/rundale-bench/split_holdout.py v1 gaeilge
python3 parish/testing/rundale-bench/build_manifest.py v1
python3 parish/testing/rundale-bench/test_grade.py
python3 -m py_compile parish/testing/rundale-bench/rundale_bench.py parish/testing/rundale-bench/grade.py parish/testing/rundale-bench/test_grade.py
python3 parish/testing/rundale-bench/rundale_bench.py --help
python3 -c 'import sys; from pathlib import Path; sys.path.insert(0, str(Path("parish/scripts/local-eval"))); from eval_lib import load_slice; print(len(load_slice("gaeilge", version="v1", split="dev"))); print(len(load_slice("gaeilge", version="v1", split="holdout")))'
just --list | rg gaeilge
vllm-mlx serve mlx-community/Qwen2.5-14B-Instruct-4bit --port 8000 --enable-prefix-cache --continuous-batching
just eval-gaeilge "mlx-community/Qwen2.5-14B-Instruct-4bit@http://localhost:8000/v1" 1
just eval-gaeilge "mlx-community/Qwen2.5-14B-Instruct-4bit@http://localhost:8000/v1"
python3 parish/testing/rundale-bench/build_leaderboard_page.py
python3 -c 'import json,re; from pathlib import Path; h=Path("docs/proofs/rundale-bench/leaderboard.html").read_text(); d=json.loads(re.search(r"<script type=\"application/json\" id=\"bench-data\">\s*(.*?)\s*</script>", h, re.S).group(1)); print(len(d.get("gaeilge", []))); print(d["gaeilge"][0]["candidate"]); print(d["gaeilge"][0]["overall"])'
cmp -s docs/proofs/rundale-bench/leaderboard.html docs/proofs/rundale-bench/leaderboard.md
git diff --check
```

## Results

- `split_holdout.py` reported `gaeilge: 11 dev / 1 holdout (8.3%)`.
- `build_manifest.py` rewrote `MANIFEST.json` and registered both source-backed `gaeilge.jsonl` and `gaeilge.holdout.jsonl`.
- `test_grade.py` reported `30/30 passed`, including the new Gaeilge grader tests.
- `py_compile` completed with no syntax errors.
- CLI help lists `gaeilge` among the `--slice` choices.
- `load_slice("gaeilge", split="dev")` printed `11`; `split="holdout"` printed `1`.
- `just --list` exposes `eval-gaeilge TARGET LIMIT=""`.
- The local MLX server loaded `mlx-community/Qwen2.5-14B-Instruct-4bit` and exposed `http://localhost:8000/v1`. The sandboxed launch could not access Metal, so the actual MLX server and bench run were executed outside the sandbox.
- The one-record smoke run completed with `records=1 errors=0`.
- The full dev run completed with `records=11 errors=0 english_leakage_flag_rate=0.091 fluency_mean=2.091 grammar_mean=2.273 idiom_mean=2.091 task_fulfillment_mean=1.909 english_leakage_mean=4.818 overall_mean=2.109`.
- The full dev run wrote `docs/proofs/rundale-bench/run_mlx_community_Qwen2_5_14B_Instruct_4bit_gaeilge_20260518T174855Z.json`.
- `build_leaderboard_page.py` reported `gaeilge=1` and regenerated both `docs/proofs/rundale-bench/leaderboard.html` and `docs/proofs/rundale-bench/leaderboard.md`.
- The embedded dashboard data smoke printed `1`, `mlx-community/Qwen2.5-14B-Instruct-4bit`, and `2.11`.
- `cmp` exited successfully, confirming `leaderboard.md` is a byte-identical embedded copy of the generated HTML dashboard.
- `git diff --check` reported no whitespace errors.

## Criteria mapping

- Bench runner integration: CLI help includes `gaeilge` and `all` includes the slice in `rundale_bench.py`.
- Checked-in corpus: `parish/testing/rundale-bench/v1/gaeilge.jsonl` has 11 dev records and `gaeilge.holdout.jsonl` has 1 holdout record after deterministic split. Records carry Tatoeba sentence IDs/contributors or UD Irish-IDT `sent_id` provenance.
- Manifest enforcement: the load-slice smoke loaded both splits through `eval_lib.load_slice`, which verifies `MANIFEST.json` hashes.
- Pinned judge rubric: `judge_gaeilge_v1.json` hash validation passes; the tamper test fails as expected.
- JSON artifact path: the runner uses the existing `docs/proofs/rundale-bench/run_<target>_<slice>_<UTC>.json` artifact path for all absolute slices, including `gaeilge`.
- Local MLX model run: `just eval-gaeilge` successfully drove the dialogue MLX model through the Gaeilge slice and produced a benchmark JSON artifact with zero candidate or judge errors.
- Dashboard integration: `build_leaderboard_page.py` ingests `run_*_gaeilge_*.json`, stores a `gaeilge` payload row, renders the 1-5 Gaeilge axes, and mirrors the generated HTML into `leaderboard.md`.
- Local tests: `30/30 passed`.
