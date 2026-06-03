# rundale-bench - Technical Debt

## Open

| ID     | Category             | Severity | Location                                                | Description                                                                                                                                                                                                                                                                                                                         |
| ------ | -------------------- | -------- | ------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| TD-001 | Complexity           | P1       | `rundale_bench.py:1-1163`                               | The main orchestrator owns CLI parsing, target/catalog resolution, all slice runners, judge dispatch, artifact writing, ingest/finalize, and aggregation in one file. Split per-slice runners, CLI commands, artifact I/O, and ingest/finalize into modules before adding more benchmark phases.                                    |
| TD-002 | Complexity           | P2       | `build_site_data.py:1-753`                              | Static-site data aggregation mixes artifact discovery, proof-run fallback discovery, model/provider/family/price enrichment, demo-profile cost shaping, and leaderboard row generation in one file. Split source discovery, catalog enrichment, profile enrichment, and output shaping so the site data contract is easier to test. |
| TD-003 | Generated Data Drift | P2       | `bench-site/src/data/bench.json:1-11690`                | The checked-in site data is a large generated artifact with no cheap freshness gate in the normal unit-test path. Add a deterministic `build_site_data.py --check` or test fixture that fails when committed site data is stale relative to its intended input directories.                                                         |
| TD-004 | Config Schema        | P2       | `candidates_local_mlx.toml:1-730`                       | The local MLX fleet catalog carries model IDs, quantization metadata, RAM estimates, and skip thresholds by hand. Add a schema/consistency test for unique IDs, required fields, positive RAM estimates, and model-name/provider compatibility before more local candidates are added.                                              |
| TD-005 | Weak Tests           | P2       | `local_runner.py:1-465`                                 | The MLX runner performs memory headroom checks, starts/stops `mlx_lm.server`, polls readiness, skips overlarge models, and appends local leaderboard rows, but no unit tests import it. Extract pure planning/readiness/result-shaping helpers so OOM-safety and skip behavior can be tested without launching MLX.                 |
| TD-006 | Stale Docs           | P3       | `README.md:11`, `AGENTS.md:45`, `v1/MANIFEST.json:4-67` | Docs still say the v1-dev dataset has 155 prompts, but `MANIFEST.json` currently records 309 dev+holdout records. Update the prose or generate the count from the manifest so benchmark status notes stay trustworthy.                                                                                                              |

## In Progress

_(none)_

## Done

_(none)_

## Progress Log

- 2026-05-25 - Initialized the rundale-bench debt ledger after scanning the Python bench runner, static-site pipeline, v1 dataset/config files, local MLX runner, and bench-site data artifact.
