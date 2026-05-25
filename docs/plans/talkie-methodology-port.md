# Porting Talkie's training methodology into the Rundale dialogue fine-tune

**Status:** design proposal · **Branch:** `claude/evaluate-talkie-llm-rLn9H` · **Date:** 2026-05-24

Companion to [`gemma4-rundale-training-plan.md`](gemma4-rundale-training-plan.md). That document specifies a QLoRA fine-tune of `google/gemma-4-9b-it` for 1820s Hiberno-English NPC dialogue. This document evaluates the newly released **Talkie-1930** model and recommends which of its *training methods* to fold into that plan, under a set of tightened constraints.

## What Talkie is

On 27 April 2026, Radford et al. released **Talkie-1930-13B**: a 13B Apache-2.0 LLM pretrained on 260B tokens of *exclusively pre-1931 English text*, plus an instruction-tuned variant `talkie-1930-13b-it` whose instruction data was mined from period reference works — etiquette manuals, letter-writing manuals, dictionaries, encyclopedias, cookbooks, and poetry/fable collections.

We are **not** adopting Talkie's weights as the Rundale base. Gemma 4 9B IT remains the base. What is valuable is Talkie's *methodology*, parts of which port cleanly into our pipeline.

## Constraints shaping this proposal

1. **No hand-written anchor set.** The 500 hand-authored examples (weighted 3×) in the original plan are dropped — we lack 1820s Hiberno-English authoring expertise to produce reliable gold rows.
2. **No frontier-API judge.** Any preference-tuning stage must use a cheaper judge than Anthropic/OpenAI.
3. **Methods, not weights.** Copy Talkie's approach; keep Gemma as the base.
4. **Cloud training, not local.** Train on a hosted GPU service rather than local hardware.

## Talkie's transferable methods

1. **Hard period cutoff at the corpus level** — modernity isn't in the prior, so it can't leak into output.
2. **Instruction-response pairs mined from period reference works** — programmatic supervision, no hand authoring.
3. **Preference tuning with a model judge** — Talkie used Claude; the *pattern* is what matters, the judge is swappable.
4. **Separation of pretraining → SFT → preference tuning** as distinct, individually-evaluable stages.

(1), (2), and (3) port cheaply at single-fine-tune scale. (4) is structural staging advice.

## Cloud training host

The base plan already lists **RunPod (A100-80GB, ~$1.89/h)** as a fallback. We promote it to the primary path. CUDA + bitsandbytes + axolotl is the most battle-tested 9B QLoRA stack, and the plan's existing `qlora_gemma4_9b.yaml` runs there unchanged.

### Why RunPod over alternatives

| Service | Hourly | Pros | Cons |
|---|---|---|---|
| **RunPod A100-80GB** (recommended) | ~$1.89 | Pay-per-second, persistent network volume (~$0.10/GB/mo), ssh + Jupyter, mature CUDA tooling, already in the plan | Build the image; cold-start ~2 min |
| Lambda Labs A100-40GB | ~$1.10 | Cheaper hourly, simpler ops | 40 GB tighter for DPO triple co-residency; less flexible storage |
| Modal (A100) | ~$2-3 | Excellent DX — `@modal.function(gpu="A100")`; orchestrate from a laptop; auto-scales | Costs more; fewer fine-tuning examples |
| Google Colab Pro+ | ~$50/mo | Notebook-first, low friction | A100 sometimes unavailable; 24 h runtime cap; disconnect risk across a multi-stage pipeline |
| Vast.ai | ~$0.40-1.00 | Cheapest A100s available | Reliability varies; spot-style availability |
| HF AutoTrain / Together / Fireworks | per-token | Zero-ops, upload JSONL → trained LoRA | Locked pipeline, **no DPO with custom local judges** — limits us to Port 1 only |

**Pick RunPod.** Cheapest option that is still flexible enough for the dual-judge DPO stage, which the managed services can't host. Modal is a fine secondary if scriptable orchestration matters more than ~$15.

### Memory budget on A100-80GB

80 GB of dedicated VRAM lets the actor, frozen reference, and both judges be **co-resident** during DPO scoring with comfortable margin:

| Component | Footprint | Notes |
|---|---|---|
| Gemma 4 9B base (NF4) | ~5 GB | bitsandbytes 4-bit |
| LoRA adapters + grads + paged 8-bit optimizer | ~1 GB | r=16, 7 modules |
| Activations (seq 4096, mb 4, ckpt) | ~10 GB | fits easily on 80 GB |
| **SFT subtotal** | **~16 GB** | |
| DPO frozen reference (NF4) | ~5 GB | inference only |
| Talkie-1930-13B-IT at q4 (period judge) | ~7.5 GB | inference only |
| Qwen 3.5 9B at q4 (coherence judge) | ~5.5 GB | inference only |
| **DPO co-residence** | **~34 GB** | well under 80 GB |

The base plan's "tight at 12-14 GB on RX 9070 16 GB at seq 1536" risk disappears. We run the RunPod tier (`gemma4-rundale-training-plan.md:108` — seq 4096 / mb 4 / grad-accum 4).

### Throughput and cost

A100-80GB on Gemma 4 9B QLoRA at seq 4096 / mb 4 / grad-accum 4: ~6 h for 3 epochs on ~100k spans. DPO adds 2-3 h. Total wall-clock ~9 h × $1.89 ≈ **$17 per clean run**. A 100 GB persistent volume kept between runs adds ~$10/mo if you iterate.

## How the plan changes under no-anchor + cheap-judge

### No anchor set → curation pipeline becomes load-bearing

Without 500 hand-written rows as stylistic ground truth, all style signal comes from extracted dialogue and mined reference pairs. Several modules move onto the critical path:

- **`curate/dialogue_extractor.py`** must produce clean speaker-attributed spans, not just regex hits.
- **`curate/feature_tagger.py`** must *gate*, not just label: drop cottier-class spans with zero substrate features.
- **`curate/class_assigner.py`** must require explicit class evidence (verb-of-saying speaker → known-class lookup, OR substrate-feature density above threshold).
- **`curate/joyce_pairs.py`** is promoted to *primary labeled signal* — Joyce 1910 explicitly defines dialect forms ("X — i.e. Y"), our only gold-standard supervision.

### Talkie *as judge* replaces Claude *as judge*

Talkie's pre-1931 prior IS the period-appropriateness rubric. Use it as a **log-likelihood scorer** (one forward pass per candidate) under a fixed Roscommon-1820s system prompt. Open-weights, runs on the same pod at q4 (~7.5 GB), zero marginal cost.

For the orthogonal "is this dialogue *coherent* / in-character / mood-appropriate" axis, Talkie's general reasoning is capped. Use **Qwen 3.5 9B at q4** as the coherence judge — a different model family from the Gemma actor, so judge-actor correlation is mitigated. Also runs on the same pod.

The original "train a small period-only LM as a perplexity oracle" idea is obsolete — Talkie already is that oracle, at 13B instead of 1B.

### Data mix without anchors

| | Original | Revised |
|---|---|---|
| Literary-extracted dialogue (Joyce/Griffin/Carleton/Croker/Kickham) | 70% | **75%** |
| Joyce 1910 dialect↔standard pairs | 25% | **20%** |
| Reference-work instruction pairs (NEW — Talkie-style) | — | **5%** |
| Hand-written anchor (3× weighted) | 5% | **0%** (removed) |

## Recommendation — two methodology ports

Both are compatible with cloud + no anchors + no API judges.

### Port 1 (cheap, high impact): reference-work instruction pairs

New mining stage alongside `instruction_pairs.py`. Sources:

- **Joyce, *English As We Speak It in Ireland* (1910)** — already ingested; glossary entries → bidirectional `"What does X mean?"` pairs.
- **Period etiquette + letter-writing manuals** (Internet Archive) — gentry / middling-farmer register.
- **Period almanacs / *Old Moore's Almanack*** — situated knowledge across classes.
- **Period dictionaries** filtered to the game's domain.

Implementation: `training/src/parish_train/build/reference_pairs.py`. Rows tagged `meta.source = "reference-work"`. Target: 3-5k pairs.

### Port 2 (moderate cost, high impact): DPO with local dual judges on the same pod

After SFT:

1. Generate N=4 candidates per held-out scenario from the SFT model (~1200 generations, ~30 min on the same A100).
2. Score each candidate:
   - **Period axis:** `talkie-1930-13b-it` log-likelihood under a fixed Roscommon-1820s system prompt.
   - **Coherence axis:** `qwen3.5:9b` 1-10 rating against the Hiberno-English rubric.
3. Combined rank → DPO (chosen, rejected) pairs.
4. Short DPO pass via axolotl's DPO support (~2-3 h on A100).

No frontier-API spend at any stage. Marginal cost: ~$5 of additional GPU time over SFT.

## What stays untouched

- Base model: **Gemma 4 9B IT**.
- **axolotl + bitsandbytes NF4 QLoRA** training stack (the plan's existing choice — runs unchanged on RunPod).
- LoRA → fp16 merge → GGUF q4_K_M → Ollama packaging path.
- Ollama serving via the `gemma4-rundale:9b` artifact.
- Tier 1 JSON schema + streaming dialogue extraction (`crates/parish-npc/src/lib.rs`, `crates/parish-types/src/ids.rs::extract_dialogue_from_partial_json`).
- The `/prove rundale-dialect` gate.

## Files to create / modify

**Create:**

- `training/src/parish_train/ingest/{ia_etiquette,ia_letter_writing,ia_almanac,ia_period_dict}.py`
- `training/src/parish_train/build/reference_pairs.py` — reference-work pair miner
- `training/src/parish_train/eval/judge_talkie.py` — log-likelihood scorer (local Talkie via Ollama)
- `training/src/parish_train/eval/judge_qwen.py` — coherence judge (local Qwen 3.5 9B via Ollama)
- `training/src/parish_train/eval/build_dpo_dataset.py` — combines both judges → DPO pairs
- `training/configs/dpo_gemma4_rundale.yaml` — axolotl DPO config
- `training/scripts/runpod_setup.sh` — pod bootstrap (CUDA, bitsandbytes, axolotl, ollama, model pulls)
- `training/scripts/run_runpod_full.sh` — orchestrates SFT → judge calibration → DPO → eval

**Modify in `gemma4-rundale-training-plan.md`:**

- *Decisions:* strike the hand-written anchor row; record the no-anchor decision, the cheap-judge constraint, and that **RunPod is now the primary host (not fallback)**.
- *Data ingestion:* add etiquette / letter-writing / almanac / dictionary rows.
- *Instruction-pair construction:* replace the 70/25/5 mix with 75/20/5; remove anchor weighting.
- *Data curation:* elevate `feature_tagger.py` to a *gate* and `class_assigner.py` to evidence-based assignment.
- *Training stack:* insert a "Stage 2: DPO with local dual judges (Talkie + Qwen)" subsection between SFT and packaging.
- *Hardware fit check:* replace the RX 9070 budget table with the A100-80GB co-residence table above.
- *Evaluation:* add the dual-judge scoring scheme as both training signal and regression sensor.
- New *Methodology lineage* section: credit Talkie for the reference-work-mining and judge-model patterns; note explicitly that we use Talkie as the judge, not as the base.

**Modify elsewhere:**

- `docs/research/Irish-English-1820s-resources.md` — add reference-work sources.
- `docs/plans/llm-quality-evals.md` — note the dual-judge harness; downgrade the Anthropic referee from required to optional.
- `docs/design/ai-techniques/03-dialogue-quality-loops.md` — flag Talkie-as-judge as a candidate inference-time rejection sampler too.

No Rust code changes. No runtime serving changes.

## Verification — RunPod runbook

```sh
# === LOCAL (laptop) — build the data, no GPU needed ===
cd training
uv sync
uv run python -m parish_train.ingest.gutenberg_joyce
uv run python -m parish_train.ingest.ia_griffin
uv run python -m parish_train.ingest.gutenberg_carleton
uv run python -m parish_train.ingest.ia_croker
uv run python -m parish_train.ingest.gutenberg_kickham
uv run python -m parish_train.ingest.ia_etiquette
uv run python -m parish_train.ingest.ia_letter_writing
uv run python -m parish_train.ingest.ia_almanac
uv run python -m parish_train.curate.dialogue_extractor
uv run python -m parish_train.curate.feature_tagger     # now a gate
uv run python -m parish_train.curate.dedupe
uv run python -m parish_train.build.instruction_pairs
uv run python -m parish_train.build.reference_pairs     # new
uv run python -m parish_train.build.split
tar czf payload.tgz data/processed configs eval/calibration_pairs.jsonl
runpodctl send payload.tgz                              # or scp to the pod

# === RUNPOD (A100-80GB pod, ~$1.89/h, 100 GB persistent volume) ===
# Template: "PyTorch 2.x + CUDA 12.x"
ssh root@<pod>
bash scripts/runpod_setup.sh                            # CUDA, axolotl, ollama, model pulls
ollama pull qwen3.5:9b
huggingface-cli download radford-et-al/talkie-1930-13b-it --local-dir models/talkie-it
bash src/parish_train/package/to_gguf.sh models/talkie-it talkie-1930-13b-it
ollama create talkie-1930-13b-it -f - <<< "FROM ./models/talkie-1930-13b-it-q4_K_M.gguf"

# Calibrate judges on the 50-pair set (~10 min). Green bar: ≥80% direction-correct per axis.
python -m parish_train.eval.judge_talkie --calibration eval/calibration_pairs.jsonl
python -m parish_train.eval.judge_qwen   --calibration eval/calibration_pairs.jsonl

axolotl train configs/qlora_gemma4_9b.yaml              # SFT (~6 h)

python -m parish_train.eval.build_dpo_dataset --sft-model models/sft-out --n-candidates 4
axolotl train configs/dpo_gemma4_rundale.yaml           # DPO (~2-3 h)

python -m parish_train.eval.ab_compare \
    --base models/sft-out --candidate models/dpo-out --judge talkie+qwen

python -m parish_train.package.merge_lora
bash src/parish_train/package/to_gguf.sh models/merged-fp16 gemma4-rundale
runpodctl receive models/gemma4-rundale-q4_K_M.gguf     # back to laptop
# Stop the pod (storage persists at ~$0.10/GB/mo if iterating)

# === LOCAL ===
ollama create gemma4-rundale:9b -f training/configs/modelfile.gemma4-rundale
/prove rundale-dialect
```

**Cost per clean run:** ~9 h × $1.89 ≈ **$17**, plus ~$10/mo if the persistent volume is kept.

**Green bar to merge:**

1. Both judge calibrations show ≥80% direction-correct on the 50-pair set.
2. Reference-work pairs appear in `data/processed/train.jsonl`; the rubric improves versus an SFT-only, no-anchor / no-reference control.
3. The DPO model beats the SFT model on the dual-judge A/B for ≥60% of paired prompts.
4. The base plan's existing merge gates (`gemma4-rundale-training-plan.md:238-246`) still pass: ≥1 substrate feature / 30 tokens on the cottier slice, ≤0.05 anachronism rate, green `/prove rundale-dialect`.

## Sources

- [Simon Willison — Introducing talkie: a 13B vintage language model from 1930](https://simonwillison.net/2026/Apr/28/talkie/)
- [MarkTechPost — Meet Talkie-1930: A 13B Open-Weight LLM Trained on Pre-1931 English Text](https://www.marktechpost.com/2026/04/27/meet-talkie-1930-a-13b-open-weight-llm-trained-on-pre-1931-english-text-for-historical-reasoning-and-generalization-research/)
- [The Decoder — What an LLM that knows nothing after 1930 thinks 2026 looks like](https://the-decoder.com/here-is-what-an-llm-that-knows-nothing-after-1930-thinks-our-world-looks-like-in-2026/)
- [The Register — Vintage chatbot lives in the past](https://www.theregister.com/2026/04/28/vintage_chatbot_lives_in_past/)
