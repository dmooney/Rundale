# Inference evaluation scripts (local + cloud)

Lightweight Python scripts that drive any OpenAI-compatible chat-completion
endpoint with Parish's production prompt fixtures. Used to spot-check
dialogue quality, detect non-Latin script leakage, and produce the data
that backs `parish-config::presets::preset_models()` — replacing guessed
model picks with measured winners.

All scripts share `eval_lib.py`, which defines the `Target` abstraction:

```text
model@base_url                          # local, no auth
model@base_url#env:VAR                  # cloud, API key in $VAR
```

| Script           | Purpose                                                                                                                                                                                                                                                                                                                                              | Output                                          |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------- |
| `gen_samples.py` | Renders **2 examples per inference category** (Intent, Reaction, Tier 2 Sim, Tier 3 Sim, Dialogue) using the production prompt builders mirrored from [`parish-inference/examples/inf_bench.rs`](../../crates/parish-inference/examples/inf_bench.rs). Takes `--small` and `--large` target specs; defaults reproduce the two-slot vllm-mlx loadout. | `docs/proofs/local-perf/category_samples.md`    |
| `flaw_scan.py`   | Runs **N dialogue prompts** through one target and flags responses containing non-Latin scripts (Cyrillic, Han, Hiragana, Katakana, Hangul, Arabic, Hebrew, Greek, Devanagari), empty output, or > 800 char output. `--prompts` / `--workers` configurable.                                                                                          | `docs/proofs/local-perf/dialogue_flaw_scan*.md` |
| `gen_dlg.py`     | 5-prompt canonical dialogue sampler — paired with `/rundale-bench eval-dialogue` for blind A/B/N. Takes a single target spec + output path.                                                                                                                                                                                                          | path supplied on CLI                            |
| `eval_lib.py`    | Shared `Target`, `parse_target`, `call_chat`, `CostTracker`, and `COSTS` table.                                                                                                                                                                                                                                                                      | —                                               |

For blind-judge quality scoring across two or more models, prefer the
[`/rundale-bench eval-dialogue`](../../../.agents/skills/rundale-bench/SKILL.md)
Claude slash command — it spawns the judge + candidates, runs the 5-axis rubric,
and archives a scoring report under the ignored local archive at
`docs/proofs/local-perf/` in one go.

## Prerequisites

### Local (Apple Silicon)

1. `uv tool install vllm-mlx`.
2. Spawn the slots before running. Example two-slot loadout:

   ```sh
   vllm-mlx serve mlx-community/Qwen2.5-7B-Instruct-4bit \
       --port 8000 --enable-prefix-cache --continuous-batching &
   vllm-mlx serve mlx-community/Qwen2.5-1.5B-Instruct-4bit \
       --port 8001 --enable-prefix-cache --continuous-batching &
   ```

### Cloud

Set the relevant `PARISH_*_API_KEY` env var (matches Parish runtime
conventions) and reference it in the target spec via `#env:VAR`:

```sh
export ANTHROPIC_API_KEY=sk-ant-...
export PARISH_GROQ_API_KEY=gsk-...
export PARISH_OPENAI_API_KEY=sk-...
```

## Running the scripts

```sh
# Local two-slot vllm-mlx — defaults
python3 parish/scripts/local-eval/gen_samples.py

# Cross-provider: Groq small slot, Claude large slot
python3 parish/scripts/local-eval/gen_samples.py \
    --small 'llama-3.1-8b-instant@https://api.groq.com/openai/v1#env:PARISH_GROQ_API_KEY' \
    --large 'claude-sonnet-4-6@https://api.anthropic.com/v1#env:ANTHROPIC_API_KEY'

# Cloud flaw scan, lower concurrency to stay under rate limits
python3 parish/scripts/local-eval/flaw_scan.py \
    --target 'claude-sonnet-4-6@https://api.anthropic.com/v1#env:ANTHROPIC_API_KEY' \
    --prompts 25 --workers 2

# Blind dialogue sample for one target (used by /eval-dialogue)
python3 parish/scripts/local-eval/gen_dlg.py \
    'mlx-community/Qwen2.5-14B-Instruct-4bit@http://localhost:8000/v1' \
    /tmp/dlg-qwen14.txt
```

Every run prints a `cost: N calls, X in + Y out tokens, ~$Z.ZZZZ` footer
sourced from each call's `usage` block. Static $/M-token rates live in
`eval_lib.py::COSTS` — verify before treating totals as gospel.

## Why keep these alongside the proof bundle?

The locally archived bench results at `docs/proofs/local-perf/evidence.md` are
only as good as the prompts that produced them. Keeping the scripts checked in
alongside the code that produces the archive means:

- a future maintainer can re-run the same evaluation on a new model
  without re-deriving the prompts;
- prompt drift between the bench (`inf_bench.rs`) and the eval scripts
  is visible at PR review time;
- the flaw-scan rate (% non-Latin script leakage) becomes a comparable
  metric across model swaps.

If you change a production prompt builder, mirror the change here.
