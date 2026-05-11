# Local inference evaluation scripts

Lightweight Python scripts that drive a running vllm-mlx (or any
OpenAI-compatible) server with Parish's production prompt fixtures.
Used to spot-check dialogue quality and detect non-Latin script
leakage when swapping the macOS local-runtime model tier.

| Script | Purpose | Output |
|---|---|---|
| `gen_samples.py` | Renders **2 examples per inference category** (Intent, Reaction, Tier 2 Sim, Tier 3 Sim, Dialogue) using the production prompt builders mirrored from [`parish-inference/examples/inf_bench.rs`](../../crates/parish-inference/examples/inf_bench.rs). Hits the two-slot loadout (small slot on `:8001`, large on `:8000`). | `docs/proofs/local-perf/category_samples.md` |
| `flaw_scan.py` | Runs **100 dialogue prompts** through the Dialogue slot and flags responses containing non-Latin scripts (Cyrillic, Han, Hiragana, Katakana, Hangul, Arabic, Hebrew, Greek, Devanagari), empty output, or > 800 char output. | `docs/proofs/local-perf/dialogue_flaw_scan*.md` |
| `gen_dlg.py` | Streaming-SSE 5-prompt dialogue sampler (older, kept for ad-hoc blind A/B compares with a single command-line `<model> <out_path>` invocation). | path supplied on CLI |

For blind-judge quality scoring across two or more models, prefer the
[`/eval-dialogue`](../../../.agents/skills/eval-dialogue/SKILL.md) Claude
slash command — it spawns the judge + candidates, runs the 5-axis rubric,
and archives a scoring report under `docs/proofs/local-perf/` in one go.

## Prerequisites

1. `uv tool install vllm-mlx` (Apple Silicon only).
2. Pull the model(s) under test; first launch will fetch from
   Hugging Face. Example for the current two-slot loadout:
   ```sh
   # large slot (Dialogue)
   vllm-mlx serve mlx-community/Qwen2.5-7B-Instruct-4bit \
       --port 8000 --enable-prefix-cache --continuous-batching &
   # small slot (Intent / Reaction / Simulation)
   vllm-mlx serve mlx-community/Qwen2.5-1.5B-Instruct-4bit \
       --port 8001 --enable-prefix-cache --continuous-batching &
   ```

## Running the scripts

```sh
# Production-faithful 2-per-category samples
python3 parish/scripts/local-eval/gen_samples.py

# 100-prompt flaw scan (defaults to large slot on :8000)
python3 parish/scripts/local-eval/flaw_scan.py

# 5-prompt dialogue sampler — useful for blind compares between models
python3 parish/scripts/local-eval/gen_dlg.py \
    mlx-community/Qwen2.5-14B-Instruct-4bit \
    /tmp/dlg-qwen14.txt
```

Each script edits the model name + output path at the top of the file.
When benchmarking a new tier (Qwen2.5-14B, future Qwen3.x, etc.), copy
the script, change the constants, run, archive the output under
`docs/proofs/local-perf/`.

## Why keep these alongside the proof bundle?

The bench results in
[`docs/proofs/local-perf/evidence.md`](../../../docs/proofs/local-perf/evidence.md)
are only as good as the prompts that produced them. Keeping the scripts
checked in next to the proof bundle means:

- a future maintainer can re-run the same evaluation on a new model
  without re-deriving the prompts;
- prompt drift between the bench (`inf_bench.rs`) and the eval scripts
  is visible at PR review time;
- the flaw-scan rate (% non-Latin script leakage) becomes a comparable
  metric across model swaps.

If you change a production prompt builder, mirror the change here.
