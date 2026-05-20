Evidence type: gameplay transcript

# rundale-bench Root Layout Evidence

## Commands run

```sh
python3 rundale-bench/test_grade.py
python3 -m py_compile rundale-bench/rundale_bench.py rundale-bench/grade.py rundale-bench/test_grade.py rundale-bench/build_leaderboard_page.py parish/scripts/local-eval/eval_lib.py
python3 -c 'import sys; from pathlib import Path; sys.path.insert(0, str(Path("parish/scripts/local-eval"))); from eval_lib import load_slice; print(len(load_slice("gaeilge", version="v1", split="dev"))); print(len(load_slice("gaeilge", version="v1", split="holdout")))'
python3 rundale-bench/rundale_bench.py --help
just --list | rg gaeilge
just --justfile parish/justfile --list | rg gaeilge
python3 rundale-bench/build_leaderboard_page.py
python3 -c 'from pathlib import Path; text = Path("rundale-bench/artifacts/leaderboard.md").read_text(); print("# rundale-bench v1 leaderboard" in text); print("mlx-community/Qwen2.5-14B-Instruct-4bit" in text); print(any(s in text for s in ["<script", "<style", "<!DOCTYPE"]))'
rg -n "rundale-bench/artifacts/leaderboard.md" README.md
rg -n "parish/testing/rundale-bench|docs/proofs/rundale-bench|testing/rundale-bench|rundale-bench/artifacts-root-layout" README.md justfile parish/justfile parish/scripts/local-eval rundale-bench docs/agent docs/plans docs/proofs/gaeilge-fluency-eval
git diff --check
```

## Results

- `test_grade.py` reported `30/30 passed`.
- `py_compile` completed with no syntax errors.
- `eval_lib.load_slice("gaeilge", split="dev")` printed `11`; `split="holdout"` printed `1`, proving the shared loader resolves the root-level `rundale-bench/v1/` corpus.
- `rundale_bench.py --help` showed the runner from `rundale-bench/rundale_bench.py` and listed `gaeilge` among the valid slices.
- Root `just --list` exposed `eval-gaeilge TARGET LIMIT=""`.
- `just --justfile parish/justfile --list` exposed the Parish wrapper recipe for `eval-gaeilge`.
- `build_leaderboard_page.py` wrote `rundale-bench/artifacts/leaderboard.html + rundale-bench/artifacts/leaderboard.md (quality=57 perf=30 gaeilge=1 cached=30 unjudged=1)`.
- The Markdown smoke printed `True`, `True`, `False`, confirming the generated Markdown has the expected heading and Gaeilge row and no raw dashboard HTML tags.
- The README link check found `rundale-bench/artifacts/leaderboard.md` on line 9.
- The old-path scan returned no matches across current scripts, bench code, README, agent docs, plan docs, and the active Gaeilge proof bundle.
- `git diff --check` reported no whitespace errors.

## Criteria mapping

- Root code/corpus layout: `rundale-bench/` now contains the benchmark scripts and `v1/` corpus.
- Artifact layout: generated runs, cached dialogue samples, perf files, ELO files, and the leaderboard pages now live under `rundale-bench/artifacts/`.
- Script path updates: runner, cache, perf, multi-axis scorer, rubric lab, manifest/split helpers, leaderboard generator, `eval_lib`, and just recipes resolve the new structure.
- README visibility: root `README.md` prominently links to `rundale-bench/artifacts/leaderboard.md` and the root harness directory.
