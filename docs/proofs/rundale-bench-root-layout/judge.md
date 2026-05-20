Verdict: sufficient
Technical debt: clear
Acceptance criteria: met

The layout change satisfies the stated goal. `rundale-bench` is now a root-level tool directory with its source files and `v1` corpus together, while generated benchmark output is under `rundale-bench/artifacts/`.

The path updates are covered by smoke tests rather than only inspection: `eval_lib.load_slice()` loads the Gaeilge dev and holdout splits from the new corpus location; `rundale_bench.py --help` executes from the root-level runner; the root and Parish `eval-gaeilge` just recipes are discoverable; and the leaderboard generator rewrites both `leaderboard.html` and the GitHub-renderable `leaderboard.md` under `rundale-bench/artifacts/`.

No old `parish/testing/rundale-bench` or `docs/proofs/rundale-bench` references remain in the current scripts/docs search scope, and the Python grader suite still reports `30/30 passed`.
