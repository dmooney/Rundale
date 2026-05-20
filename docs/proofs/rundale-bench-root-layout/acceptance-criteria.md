Evidence type: gameplay transcript

# Acceptance Criteria: rundale-bench-root-layout

## Task

Move the `rundale-bench` benchmark tool out of `parish/testing/` into a root-level `rundale-bench/` directory. Generated benchmark output should live under `rundale-bench/artifacts/`, and scripts/docs/recipes should resolve the new paths without relying on the old locations.

## Criteria

- The bench code and corpus live under root `rundale-bench/`, including `rundale_bench.py`, `grade.py`, `test_grade.py`, `build_leaderboard_page.py`, and `v1/` — observable via: `test -f rundale-bench/rundale_bench.py` and `test -f rundale-bench/v1/MANIFEST.json`.
- Generated benchmark outputs live under `rundale-bench/artifacts/`, including the leaderboard and existing run/sample/perf JSON artifacts — observable via: `test -f rundale-bench/artifacts/leaderboard.md` and `test -f rundale-bench/artifacts/run_mlx_community_Qwen2_5_14B_Instruct_4bit_gaeilge_20260518T174855Z.json`.
- The shared `eval_lib.load_slice()` helper resolves slices from the new root-level bench directory — observable via: a Python load-slice smoke check printing the Gaeilge dev and holdout counts.
- The bench runner and leaderboard generator write future output to `rundale-bench/artifacts/` — observable via: `python3 rundale-bench/rundale_bench.py --help` and `python3 rundale-bench/build_leaderboard_page.py`.
- The top-level and Parish just recipes for `eval-gaeilge` point at the moved runner — observable via: `just --list | rg gaeilge` and `cd parish && just --list | rg gaeilge`.
- The root README links prominently to the generated benchmark leaderboard — observable via: `rg "rundale-bench/artifacts/leaderboard.md" README.md`.

## Verification

Run:

```sh
python3 rundale-bench/test_grade.py
python3 -m py_compile rundale-bench/rundale_bench.py rundale-bench/grade.py rundale-bench/test_grade.py rundale-bench/build_leaderboard_page.py parish/scripts/local-eval/eval_lib.py
python3 -c 'import sys; from pathlib import Path; sys.path.insert(0, str(Path("parish/scripts/local-eval"))); from eval_lib import load_slice; print(len(load_slice("gaeilge", version="v1", split="dev"))); print(len(load_slice("gaeilge", version="v1", split="holdout")))'
python3 rundale-bench/build_leaderboard_page.py
python3 -c 'from pathlib import Path; text = Path("rundale-bench/artifacts/leaderboard.md").read_text(); print("# rundale-bench v1 leaderboard" in text); print("mlx-community/Qwen2.5-14B-Instruct-4bit" in text); print(any(s in text for s in ["<script", "<style", "<!DOCTYPE"]))'
just --list | rg gaeilge
cd parish && just --list | rg gaeilge
rg "parish/testing/rundale-bench|docs/proofs/rundale-bench|testing/rundale-bench|rundale-bench/artifacts-root-layout" README.md justfile parish/justfile parish/scripts/local-eval rundale-bench docs/agent docs/plans docs/proofs/gaeilge-fluency-eval
```

Expected signals in output:

- Grader tests pass.
- Python compile succeeds.
- Load-slice smoke prints `11` and `1`.
- Leaderboard generator writes `rundale-bench/artifacts/leaderboard.html + rundale-bench/artifacts/leaderboard.md`.
- Markdown smoke prints `True`, `True`, `False`.
- Just recipe discovery still exposes `eval-gaeilge`.
- The old-path `rg` check returns no matches in current docs/scripts.
