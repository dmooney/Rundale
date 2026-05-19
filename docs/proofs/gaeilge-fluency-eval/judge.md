Verdict: sufficient
Technical debt: clear
Acceptance criteria: met

The implementation satisfies the `rundale-bench` slice scope without adding a game fixture. The Gaeilge corpus now lives under `parish/testing/rundale-bench/v1/` with source provenance from Tatoeba and UD Irish-IDT, dev and holdout splits registered in `MANIFEST.json`; `rundale_bench.py` accepts `--slice gaeilge` and includes it in `--slice all`; `grade.py` has a pinned-judge Gaeilge grader; and the README / justfile entry route through the bench runner.

A live local model run was executed against `mlx-community/Qwen2.5-14B-Instruct-4bit` through the OpenAI-compatible MLX endpoint at `http://localhost:8000/v1`. The full dev split completed with 11 records, 0 errors, and wrote `docs/proofs/rundale-bench/run_mlx_community_Qwen2_5_14B_Instruct_4bit_gaeilge_20260518T174855Z.json`.

The static leaderboard path now includes the same Gaeilge artifact. `build_leaderboard_page.py` ingests `run_*_gaeilge_*.json`, emits a `gaeilge` payload, and the regenerated `leaderboard.html` renders the MLX Qwen2.5 row with the Gaeilge overall / fluency / grammar / idiom / task / English-leakage metrics. The old prose-only `leaderboard.md` content has been removed; the generator now writes a GitHub-renderable Markdown snapshot from the same data because GitHub Markdown strips the script/style tags required by the interactive HTML dashboard.
