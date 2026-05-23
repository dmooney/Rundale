# Local MLX candidate sweep — 2026-05-19

Evidence type: gameplay transcript

Sweep of MLX candidate models for the two-slot vllm-mlx layout (Apple Silicon
M5 Pro, 48 GB unified). Runs via the new
`rundale-bench/local_runner.py` orchestrator, which spawns
`mlx_lm.server` per candidate, samples peak RSS at 4 Hz during inference,
invokes the unified `rundale_bench.py` orchestrator against
`http://127.0.0.1:<port>`, and appends a leaderboard row.

## Host

- Apple M5 Pro, 48 GB unified memory
- macOS Darwin 25.4
- mlx-lm 0.31.3, mlx-metal 0.31.2 (in `/Users/dmooney/Rundale/.venv-mlx`)
- All candidates from `mlx-community/*` via the HuggingFace cache

## Judge rotation

All scores in this document were produced with `x-ai/grok-4.3` (via
OpenRouter) acting as judge — a temporary rotation away from the pinned
`qwen/qwen3-235b-a22b-2507` to avoid same-family bias against Qwen
candidates. Rubric text was unchanged so `rubric_sha256` still verified.
The rotation was reverted on the upstream rebase: the committed
`v1/judge_*.json` files remain on `qwen3-235b`. Re-running any row through
the upstream qwen judge will produce different absolute scores; the
within-sweep ordering should be stable.

## Thinking-mode fix

`parish/scripts/local-eval/eval_lib.py` was extended to inject
`chat_template_kwargs={"enable_thinking": False}` into chat-completion
requests for `mlx-community/Qwen3*` and `mlx-community/Qwen3.5*`
candidates. Without this, Qwen3 chat templates emit a `<think>…</think>`
reasoning trace that consumes the `max_tokens=200` budget; the bench
ends up scoring the model's internal monologue rather than its reply.

Effect on Qwen3 tiny slot (5-prompt dialogue, same prompts, Grok-4.3
judge):

| Model           | Thinking-leak | Thinking-fixed | Δ      |
|-----------------|---------------|----------------|--------|
| Qwen3-0.6B-4bit | overall=1.20  | overall=2.12   | +0.92  |
| Qwen3-1.7B-4bit | overall=1.24  | overall=2.84   | +1.60  |
| Qwen3-4B-4bit   | overall=1.20  | overall=4.04   | +2.84  |

Without the fix every Qwen3 score floored at ~1.2 — the language-rubric
penalty for unintelligible chain-of-thought.

## Tiny-slot results (intent + dialogue, 5-prompt dialogue dev split)

Intent uses the deterministic Jaccard grader; dialogue uses the Grok-4.3
LLM judge (`judge_v1`).

| Model                                | params_B | quant | peak_RAM_GB | intent label_match | dialogue overall |
|--------------------------------------|----------|-------|-------------|--------------------|------------------|
| mlx-community/Qwen2.5-1.5B-Instruct-4bit (incumbent) | 1.5  | 4bit | 0.29 | **0.800** | 1.64 |
| mlx-community/Qwen3-0.6B-4bit       | 0.6  | 4bit | 0.63 | 0.200 / 0.400 | 2.12 |
| mlx-community/Qwen3-1.7B-4bit       | 1.7  | 4bit | 1.30 | 0.560 / 0.720 | 2.84 |
| mlx-community/Qwen3-4B-4bit         | 4.0  | 4bit | 2.60 | 0.680 | **4.04** |
| mlx-community/Phi-4-mini-instruct-4bit | 3.8  | 4bit | 2.51 | 0.160 / 0.560 | 2.76 |
| mlx-community/Llama-3.2-3B-Instruct-4bit | 3.0  | 4bit | 2.21 | 0.720 / 0.760 | 3.32 |
| mlx-community/gemma-3-4b-it-4bit    | 4.0  | 4bit | 3.07 | 0.720 / 0.680 | **4.04** |

Two `intent` numbers are shown for some rows because two parallel sweep
processes wrote to the leaderboard concurrently — minor variance from
sampling.

### Tiny-slot recommendations

- **Intent (function-calling, JSON output):** Qwen2.5-1.5B-Instruct-4bit
  remains the leader at 0.800 label-match. None of the new candidates beat
  it. Qwen3-4B is the closest at 0.680.
- **Reaction (short in-character greetings):** Qwen3-4B-4bit or
  gemma-3-4b-it-4bit, both at 4.04 overall.
- **Combined slot:** Qwen3-4B-4bit covers both — competitive intent and
  best-in-tier dialogue, only 2.6 GB peak.

## Large-slot results (5-prompt dialogue dev split, sweep complete)

| Model                                            | params_B | quant | peak_RAM_GB\* | dialogue overall                                | $/run    |
|--------------------------------------------------|----------|-------|---------------|--------------------------------------------------|----------|
| **mlx-community/Qwen3-14B-4bit** ⭐              | 14.0     | 4bit  | 2.13          | **4.40 (c=4.0 a=4.8 l=5.0 r=4.0 cr=4.2)**       | $0.0756  |
| mlx-community/Qwen2.5-14B-Instruct-4bit (incumbent) | 14.0  | 4bit  | 2.01          | 4.12 (c=3.8 a=4.8 l=4.8 r=3.4 cr=3.8)           | $0.0697  |
| mlx-community/gemma-3-12b-it-4bit                | 12.0     | 4bit  | 7.71          | 4.12 (c=3.6 a=4.6 l=5.0 r=3.8 cr=3.6)           | $0.0746  |
| mlx-community/Qwen3-30B-A3B-4bit (MoE, 3B active) | 30.0    | 4bit  | 6.88          | 3.84 (c=3.4 a=4.2 l=4.6 r=3.6 cr=3.4)           | $0.0793  |
| mlx-community/Qwen3-8B-4bit                      | 8.0      | 4bit  | 4.95          | 3.84 (c=3.4 a=4.0 l=4.8 r=3.4 cr=3.6)           | $0.0862  |
| mlx-community/Mistral-Small-24B-Instruct-2501-4bit | 24.0   | 4bit  | 9.71          | 3.08 (c=3.0 a=3.6 l=3.4 r=3.0 cr=2.4) — bug     | $0.0680  |

\* `peak_RAM_GB` is psutil `rss` only; this **undercounts** Metal-allocated
GPU memory on Apple Silicon by a factor of 3-5×. `local_runner.py` now uses
`memory_full_info().uss` (mapped to phys_footprint on darwin) which
captures Metal buffers, but the rows above predate that fix. The on-disk
footprint for these models is roughly: 14B-4bit ≈ 9 GB, 24B-4bit ≈ 14 GB,
30B-A3B-4bit ≈ 17 GB. Re-run any candidate to populate the corrected RAM.

## Expanded sweep — 24 candidates total (Qwen3.5 / Qwen3.6 / Gemma-4 / QAT / OptiQ / MoE)

### Full dialogue table (5-prompt dev split, Grok-4.3 judge, sorted by overall)

| rank | model | params | quant | RAM | overall |
|------|-------|--------|-------|-----|---------|
| 1  | mlx-community/Qwen3.6-27B-4bit                              | 27.0 | 4bit       |  2.54 | **4.48** ⭐ |
| 2  | mlx-community/Qwen3-14B-4bit                                | 14.0 | 4bit       |  2.13 | 4.40 |
| 2  | mlx-community/gemma-3-27b-it-qat-4bit                       | 27.0 | qat-4bit   | 13.61 | 4.40 |
| 4  | mlx-community/Qwen3.5-9B-OptiQ-4bit                         |  9.0 | optiq-4bit |  7.63 | 4.36 |
| 5  | mlx-community/Qwen3.6-35B-A3B-4bit (MoE, 3B active)         | 35.0 | 4bit       |  2.98 | 4.20 |
| 6  | mlx-community/Qwen2.5-14B-Instruct-4bit (incumbent)         | 14.0 | 4bit       |  2.01 | 4.12 |
| 6  | mlx-community/gemma-3-12b-it-4bit                           | 12.0 | 4bit       |  7.71 | 4.12 |
| 8  | mlx-community/Qwen3-4B-4bit                                 |  4.0 | 4bit       |  2.60 | 4.04 |
| 8  | mlx-community/gemma-3-4b-it-4bit                            |  4.0 | 4bit       |  3.07 | 4.04 |
| 10 | mlx-community/Ministral-3-3B-Instruct-2512-4bit             |  3.0 | 4bit       |  2.35 | **4.00** ⭐ |
| 11 | mlx-community/gemma-3-4b-it-qat-4bit                        |  4.0 | qat-4bit   |  3.07 | 3.88 |
| 12 | mlx-community/Qwen3-30B-A3B-4bit                            | 30.0 | 4bit       |  6.88 | 3.84 |
| 12 | mlx-community/Qwen3-8B-4bit                                 |  8.0 | 4bit       |  4.95 | 3.84 |
| 14 | mlx-community/Qwen3.5-4B-MLX-4bit                           |  4.0 | 4bit       |  2.88 | 3.80 |
| 14 | mlx-community/EuroLLM-22B-Instruct-2512-mlx-4bit             | 22.0 | 4bit       | 13.03 | 3.80 |
| 15 | mlx-community/Qwen3-4B-Instruct-2507-4bit                   |  4.0 | 4bit       |  2.60 | 3.76 |
| 15 | stelterlab/EuroLLM-9B-Instruct-MLX-4bit                     |  9.0 | 4bit       |  5.65 | 3.40 |
| 16 | mlx-community/Llama-3.2-3B-Instruct-4bit                    |  3.0 | 4bit       |  2.21 | 3.32 |
| 17 | mlx-community/Mistral-Small-24B-Instruct-2501-4bit          | 24.0 | 4bit       |  9.71 | 3.08 (tokenizer loop) |
| 18 | mlx-community/Qwen3-1.7B-4bit (thinking-fixed)              |  1.7 | 4bit       |  1.30 | 2.84 |
| 19 | mlx-community/Phi-4-mini-instruct-4bit                      |  3.8 | 4bit       |  2.51 | 2.76 |
| 19 | mlx-community/gemma-4-31b-it-4bit                           | 31.0 | 4bit       |  2.50 | 2.76 (analysis-mode) |
| 21 | mlx-community/Qwen3-0.6B-4bit (thinking-fixed)              |  0.6 | 4bit       |  0.63 | 2.12 |
| 22 | mlx-community/Qwen2.5-1.5B-Instruct-4bit                    |  1.5 | 4bit       |  1.15 | 1.64 |
| 23 | mlx-community/Qwen3.5-27B-Claude-4.6-Opus-Distilled-MLX-4bit| 27.0 | 4bit       | 13.43 | 1.28 (Claude-distilled thinking leak) |
| 24 | mlx-community/LFM2.5-1.2B-Instruct-4bit                     |  1.2 | 4bit       |  0.87 | 0.48 (mlx-lm load timeout) |
| 25 | mlx-community/gemma-4-e4b-it-4bit                           |  4.5 | 4bit       |  0.11 | 0.00 (mlx-lm rejects 126 params) |

### Key updates from expanded sweep

- **Qwen3.6-27B-4bit still leads at 4.48** — confirmed across both sweep waves.
- **gemma-3-27b-it-qat-4bit ties Qwen3-14B at 4.40** with QAT 4bit weights. ~14 GB
  RAM (real, captured with the fixed psutil sampler).
- **Qwen3.5-9B-OptiQ-4bit at 4.36** is the dark horse — strong character +
  authenticity at only 9B parameters / 7.6 GB. OptiQ blockwise-optimised quant
  visibly beats plain 4bit at the same model family.
- **Ministral-3-3B-Instruct-2512-4bit hits 4.00 at only 3B params**, the
  strongest tiny-slot candidate. The newer 2512 release fixes the tokenizer
  regex bug that broke Mistral-Small-24B-2501.
- **Qwen3.6-35B-A3B MoE (4.20)** underperforms the dense Qwen3.6-27B (4.48)
  — MoE routing isn't picking the right experts for 1820 Hiberno-English
  midwife dialogue on this short sample. Dense-27B preferred.
- **Failures worth knowing:**
  - `Qwen3.5-27B-Claude-Opus-Distilled-MLX-4bit` (1.28) — distilled from
    Claude's thinking behaviour; emits chain-of-thought as content even with
    `chat_template_kwargs={enable_thinking:False}`. Needs a "reply directly"
    system suffix to be usable.
  - `gemma-4-31b-it-4bit` (2.76) — emits character analysis bullets instead
    of in-character dialogue. System prompt interpretation issue.
  - `gemma-4-e4b-it-4bit` (0.00) — mlx-lm rejects with "Received 126
    parameters not in model". Architecture mismatch between the upload and
    mlx-lm's expected Gemma-4 layout.
  - `Mistral-Small-24B-Instruct-2501-4bit` (3.08) — tokenizer regex bug
    causes degenerate-repeat replies. Use the 2512 Ministral variant or
    upcoming 3.1-24B-Instruct-2503 instead.
  - `LFM2.5-1.2B-Instruct-4bit` — mlx-lm load hangs (Liquid architecture
    not supported by this mlx-lm version).

### Headline findings (after expanded sweep: 25 candidates)

1. **Qwen3.6-27B-4bit (May 2026) is the new large-slot leader at overall=4.48**
   (c=4.6/a=4.6/l=5.0/r=4.2/cr=4.0). ~15 GB on-disk; fits with a tiny
   co-resident in 48 GB. **Recommend swap.**
2. **Qwen3-14B-4bit at overall=4.40** is the best 16-GB-host option,
   +0.28 over Qwen2.5-14B-4bit at the same memory footprint.
3. **Qwen3.6-35B-A3B-4bit (newest MoE, ~20 GB) scored 4.20** — better
   than the Qwen3-30B-A3B predecessor (3.84) but trailed both 27B-dense
   and 14B-dense candidates on this short sample. MoE may need a larger
   evaluation set to differentiate from the dense alternatives.
2. **Qwen3-30B-A3B-4bit underperforms Qwen3-14B-4bit by -0.56** on this
   5-prompt sample. Two interpretations: (a) MoE routing isn't picking the
   right experts for 1820-Irish midwifery prose, (b) 5 prompts isn't
   enough to stabilize. Either way, the dense 14B is the better default;
   the MoE belongs on a larger holdout sweep before swap.
3. **Mistral-Small-24B-Instruct-2501-4bit ships with a broken tokenizer**
   on the current MLX upload. Replies degenerate into 10-20× repeat loops
   (see `run_mlx_community_Mistral_Small_24B*` JSON for the transcript).
   The upstream fix is `fix_mistral_regex=True` plus the 3.1-24B-2503
   variant; both are outside this sweep. Candidate flagged in
   `candidates_local_mlx.toml`.
4. **Gemma-3-12b-it-4bit ties incumbent Qwen2.5-14B at 4.12** with 3 fewer
   GB of weights and Google's QAT 4bit (~near-lossless). A solid backup
   choice if the Qwen line ever regresses.

## How to reproduce

```sh
just -f rundale-bench/justfile local-plan
just -f rundale-bench/justfile local slot=tiny slice=intent limit=25
just -f rundale-bench/justfile local slot=tiny slice=dialogue limit=5
just -f rundale-bench/justfile local slot=large slice=dialogue limit=5
```

Candidate fleet lives in
[`rundale-bench/candidates_local_mlx.toml`](../../rundale-bench/candidates_local_mlx.toml).
Append `[[candidate]]` blocks to extend; the runner enforces a 4 GB
headroom check and skips any candidate whose declared `peak_ram_gb_est`
exceeds available unified memory.

## Tier-1 roleplay sweep (added 2026-05-19 evening)

Per-user request, the curated RP/character-tuned models. All on the dialogue dev split, 5-prompt sample, Grok-4.3 judge, peak RAM captured with the fixed `memory_full_info().uss` sampler.

| Model                                                    | params_B | quant | peak_RAM_GB | dialogue overall                                | $/run   |
|----------------------------------------------------------|----------|-------|-------------|--------------------------------------------------|---------|
| mlx-community/Violet-Lyra-Gutenberg-4bit                 | 12.0     | 4bit  | 7.30        | **4.24 (c=4.2 a=4.6 l=5.0 r=4.0 cr=3.4)**       | $0.0841 |
| mlx-community/Lumimaid-Magnum-12B                        | 12.0     | bf16  | 1.29\*      | 3.84 (c=3.6 a=3.6 l=4.8 r=4.2 cr=3.0)            | $0.0831 |
| mlx-community/Cydonia-24B-v3.1-4bit                      | 24.0     | 4bit  | 7.71        | 3.68 (c=3.4 a=4.4 l=4.4 r=3.4 cr=2.8)            | $0.0800 |
| mlx-community/magnum-v3-34b-4bit                         | 34.0     | 4bit  | 6.82        | 3.44 (c=3.2 a=4.0 l=4.6 r=3.4 cr=2.0)            | $0.0845 |
| mlx-community/MN-12B-Mag-Mell-R1-6bit                    | 12.0     | 6bit  | 7.10        | 2.92 (c=2.6 a=3.4 l=3.4 r=2.6 cr=2.6)            | $0.0868 |
| mlx-community/TheDrummer_Big-Tiger-Gemma-27B-v1_4bit     | 27.0     | 4bit  | 8.48        | **0.00 — broken** (emits only `<pad>` tokens)    | $0.0000 |
| mlx-community/magnum-v4-72b-4bit                         | 72.0     | 4bit  | —           | **skipped — OOM** (rebooted system mid-load)     | —       |
| mlx-community/Midnight-Miqu-70B-v1.5-MLX-8Bit            | 70.0     | 8bit  | —           | skipped — peak_ram_gb_est=75 GB > 48 GB host     | —       |

\* Lumimaid-Magnum-12B is bf16 (24 GB weights, not 4bit); the 1.29 GB RSS is suspiciously low and likely an mmap'd-but-not-touched read at sampling time. Treat that row's RAM column as untrusted; the model itself ran and scored.

### Headline findings — tier-1 RP

1. **No tier-1 RP model beat Qwen3.6-27B-4bit (4.48) or even Qwen3-14B-4bit (4.40)** for 1820-Irish midwife dialogue. The best of the bunch, Violet-Lyra-Gutenberg-4bit at 4.24, would rank ~tied with Qwen2.5-14B (4.12). Modern RP-tuned models are tuned for contemporary romance/adventure prose; they refuse 19th-century register and lose the language rubric.
2. **`magnum-v4-72b-4bit` OOMed and locked a 48 GB M5 Pro mid-inference** (system reboot required, 2026-05-19). The `peak_ram_gb_est` in `candidates_local_mlx.toml` was bumped from 40 → 52 GB so the runner's headroom check skips it. Same patch applied to `Midnight-Miqu-70B-v1.5-MLX-8Bit`. Don't lower the estimates without measuring on a 96 GB+ box first. Empirical rule: `peak_ram_gb_est >= params_b * 0.55 + 4` for mlx-lm 4-bit. See [LEARNINGS.md](../../../LEARNINGS.md) and [project_mlx_local_ram_ceiling.md](../../../.claude/projects/-Users-dmooney-Rundale/memory/project_mlx_local_ram_ceiling.md) (auto-memory).
3. **`TheDrummer_Big-Tiger-Gemma-27B-v1_4bit` emits only `<pad>` tokens** — same broken-Gemma-quant failure mode as `gemma-4-e4b-it-4bit`. Drop from the candidate list once `frozen=true`.
4. **Violet-Lyra and Lumimaid are the only tier-1 entries worth re-trying on the larger holdout split** when corpus growth lands — they're competitive with the cheaper Qwen3-4B (4.04) on the language axis specifically.

Per-run JSONs are under `docs/proofs/rundale-bench/run_mlx_community_*_dialogue_*.json`. Aggregate sweep records: `local_20260519T194005Z.json` (Lumimaid) → `local_20260519T195919Z.json` (magnum-v3-34b).

## Corpus growth

`v1` slices grew alongside the sweep:

| Slice    | Before | After | Δ   |
|----------|--------|-------|-----|
| dialogue | 150    | 180   | +30 |
| intent   |  30    |  42   | +12 |

Manifest rebuilt: `merkle_root_sha256 = 40870c6805bdba1bf284a31d46b69294831e5a236e937b90706b026c5ff2783c`.
