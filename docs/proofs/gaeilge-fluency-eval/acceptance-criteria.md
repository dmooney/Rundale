# Acceptance Criteria: gaeilge-fluency-eval

## Task

Add a `rundale-bench` Gaeilge fluency slice that measures the Irish-language ability of a given OpenAI-compatible model target. The slice should use the existing benchmark runner, fixed dev/holdout corpus files, manifest hash validation, a pinned judge rubric, and the standard bench JSON artifacts so scores are comparable with the other model evals.

## Criteria

- The benchmark runner accepts `--slice gaeilge` and includes `gaeilge` in `--slice all` — observable via: `python3 parish/testing/rundale-bench/rundale_bench.py --help`.
- The eval uses checked-in `rundale-bench` dev and holdout slice files with stable record IDs, task types, constraints, expected features, and source provenance from Tatoeba / UD Irish-IDT — observable via: reading `parish/testing/rundale-bench/v1/gaeilge.jsonl`, `gaeilge.holdout.jsonl`, and `GAEILGE_SOURCES.md`.
- The new slice files are registered in `parish/testing/rundale-bench/v1/MANIFEST.json` so `eval_lib.load_slice("gaeilge", version="v1", split=...)` refuses silent corpus drift — observable via: a Python load-slice smoke check.
- The eval scores each response with `judge_gaeilge_v1.json`, a pinned judge rubric over fluency, grammar, idiom, task fulfilment, English leakage, and overall quality; rubric hash drift fails before judging — observable via: `python3 parish/testing/rundale-bench/test_grade.py`.
- The eval emits per-record responses, per-axis scores, aggregate means, token/cost totals, and target metadata through the existing bench artifact path under `docs/proofs/rundale-bench/` — observable via: `rundale_bench.py --slice gaeilge --target ...`.
- The eval is discoverable through `just eval-gaeilge TARGET [LIMIT]`, which delegates to `rundale_bench.py --slice gaeilge` — observable via: `just --list | rg gaeilge`.
- The static leaderboard generator ingests `run_*_gaeilge_*.json` artifacts and renders the Gaeilge means in `leaderboard.html` — observable via: `python3 parish/testing/rundale-bench/build_leaderboard_page.py` followed by checking the embedded dashboard data / HTML for a `gaeilge` row.
- `leaderboard.md` is no longer a separate hand-maintained prose ledger or a raw HTML dashboard copy; the generator writes a GitHub-renderable Markdown snapshot from the same data as `leaderboard.html` — observable via: checking that `leaderboard.md` contains the Gaeilge row and no raw dashboard `<script>`, `<style>`, or `<!DOCTYPE>` tags.

## Verification

Run:

```sh
python3 parish/testing/rundale-bench/test_grade.py
python3 parish/testing/rundale-bench/rundale_bench.py --help
python3 -c 'import sys; from pathlib import Path; sys.path.insert(0, str(Path("parish/scripts/local-eval"))); from eval_lib import load_slice; print(len(load_slice("gaeilge", version="v1", split="dev"))); print(len(load_slice("gaeilge", version="v1", split="holdout")))'
just --list | rg gaeilge
python3 parish/testing/rundale-bench/build_leaderboard_page.py
python3 -c 'from pathlib import Path; text = Path("docs/proofs/rundale-bench/leaderboard.md").read_text(); print("# rundale-bench v1 leaderboard" in text); print("mlx-community/Qwen2.5-14B-Instruct-4bit" in text); print(any(s in text for s in ["<script", "<style", "<!DOCTYPE"]))'
```

Expected signals in output:

- `test_grade.py` includes and passes the Gaeilge grader tests.
- CLI help lists `gaeilge` among `--slice` choices.
- The load-slice smoke prints non-zero dev and holdout counts.
- `just --list` exposes `eval-gaeilge TARGET LIMIT=""`.
- `build_leaderboard_page.py` reports a non-zero Gaeilge row count when a `run_*_gaeilge_*.json` artifact is present, and `leaderboard.html` contains the MLX Qwen2.5 Gaeilge row.
- The Markdown renderability smoke prints `True`, `True`, `False`: the generated `leaderboard.md` has the expected heading and Gaeilge row, without raw dashboard HTML that GitHub Markdown strips.
