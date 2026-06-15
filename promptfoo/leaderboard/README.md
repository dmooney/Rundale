# Rundale Bench Leaderboard

`leaderboard.jsonl` is the append-only benchmark-of-record written by
`promptfoo/scripts/leaderboard.py` after each canonical promptfoo pipeline run.
The bench site (`promptfoo/bench-site/`) reads this file directly at build time.

## Preliminary rows (2026-06-15)

The first eight rows were hand-written from a preliminary evaluation run on
2026-06-15. They are **dialogue-slice-only, local-MLX/llama.cpp runs** judged
by `claude-sonnet-4-6` against the v2 dialogue rubric (n=24 records).

Differences from canonical rows produced by `leaderboard.py`:

- Only the `dialogue` category is populated; `overall` equals the dialogue
  score (the leaderboard renormalizes over whichever categories are present,
  so overall == dialogue when only dialogue ran).
- Latency figures are median-only (p95 estimated as p50 \* 1.5).
- `tokens_per_sec` is a rough 60 tok/s estimate (no perf slice was run).
- `ci95` bounds are estimated as score +/- 0.10 (no bootstrap over raw items).
- `value_score` is null (free/local models, no $/game-hour to divide by).

These rows will be superseded when a full canonical run is executed and
`leaderboard.py` appends rows for the same candidates (latest row per
candidate wins in the leaderboard display).

## Headline findings (preliminary)

- gemma-4-26B-GGUF (3.94) beats the Qwen2.5-14B baseline (3.70) by +0.24
  at 3.2s median latency — best choice for real-time NPC use.
- gemma-4-31B-GGUF scores 3.95 but at 21.5s median latency.
- gemma-4-26B with thinking ON via mlx_vlm scores 4.08 but increases
  grounding fabrication; not recommended for production.
- Qwen3.5-9B has degenerate loops on 2/24 records; not production-ready.
