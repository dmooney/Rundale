# Post-Train Gemma 4 9B for Rundale Hiberno-English Dialogue

> Parent: [Docs Index](../index.md) | Related: [Inference Pipeline](inference-pipeline.md), [Irish English Resources](../research/Irish-English-1820s-resources.md), [ADR-005](../adr/005-ollama-local-inference.md)

## Context

Parish's inference pipeline (refreshed April 2026 in [`docs/design/inference-pipeline.md`](inference-pipeline.md)) names **Gemma 4 9B** as the recommended local Tier 1 Dialogue model on the RX 9070 16 GB baseline. But the refresh flags explicitly: *"Benchmarks don't measure 1820 Irish peasant dialogue. Build a small fixture and use the `/prove` harness before committing any model to production."*

The research doc [`docs/research/Irish-English-1820s-resources.md`](../research/Irish-English-1820s-resources.md) lays out the gap: no existing LLM — gaBERT, UCCIX, Caernarfon 3B, or any cloud model — has been trained on historical Hiberno-English. For 1820s Roscommon NPC speech (Irish-substrate grammar + English orthography + code-switching), a QLoRA on public-domain primary sources is the only practical path.

This plan QLoRA-fine-tunes `google/gemma-4-9b-it` on Joyce, Griffin, Carleton, Croker, Kickham dialogue plus a ~500-example hand-written anchor set, packages the result as `gemma4-rundale:9b` for Ollama, and wires it into the Dialogue provider category. The intended outcome: a feature-flagged opt-in model that meaningfully outperforms stock Gemma 4 9B on a Hiberno-English rubric and passes a new `/prove rundale-dialect` harness.

## Decisions (user-confirmed)

- **Base model:** `google/gemma-4-9b-it` (instruction-tuned variant).
- **Hand-written anchor set:** ~500 examples, weighted 3× during training.
- **Training host:** both supported — single axolotl config, two launchers (local ROCm on RX 9070, remote RunPod A100-80GB). Local is tried first; remote is always available for clean reruns.
- **Serving:** Ollama via GGUF q4_K_M; feature-flag gated drop-in for the Dialogue category.

## Repo layout — new `training/` subproject

Kept outside the cargo workspace so it doesn't pollute `just build` / `just check`. Uses `uv` + Python 3.11.

```
training/
  pyproject.toml                    # uv project, Python 3.11
  uv.lock
  README.md                         # run instructions (mirrors §Verification below)
  .gitignore                        # data/raw/**, data/interim/**, data/processed/**, models/**, vendor/**, *.gguf
  configs/
    qlora_gemma4_9b.yaml            # axolotl config (shared local + remote)
    modelfile.gemma4-rundale        # Ollama Modelfile template
  src/parish_train/
    ingest/                         # Gutenberg + Internet Archive fetchers
    curate/                         # dialogue extraction, feature tagging, dedup
    build/                          # instruction-pair JSONL + stratified split
    eval/                           # rubric, held-out scenarios, A/B harness
    package/                        # merge_lora + GGUF conversion + Modelfile render
  data/
    raw/                            # gitignored — cached downloads (SHA-256 keyed)
    interim/                        # gitignored — extracted dialogue JSONL
    processed/                      # gitignored — final train/val/test JSONL
    handwritten/anchor.jsonl        # COMMITTED — the 500-example style anchor
    LICENSES.md                     # per-source public-domain attribution
  models/                           # gitignored — HF checkpoints, merged fp16, GGUF
  scripts/
    run_local.sh                    # end-to-end local ROCm pipeline
    run_runpod.sh                   # end-to-end remote pipeline (uploads config + data tarball)
```

## Data ingestion

All sources are US public domain (life+70 years expired). Attribution kept in `data/LICENSES.md`; downloads SHA-256-cached under `data/raw/<source>/` via a shared `common.py` helper so reruns are free.

| Source                                            | Format                                   | Module                              |
|---------------------------------------------------|------------------------------------------|-------------------------------------|
| Joyce, *English As We Speak It in Ireland* (1910) | Gutenberg HTML #34251, parsed w/ `selectolax` | `ingest/gutenberg_joyce.py`     |
| Griffin, *The Collegians* (1829)                  | Internet Archive plaintext (`internetarchive` PyPI) | `ingest/ia_griffin.py`     |
| Carleton, *Traits and Stories* (1830s)            | Gutenberg author 2498 (multiple volumes) | `ingest/gutenberg_carleton.py`      |
| Croker, *Fairy Legends* (1825)                    | Internet Archive plaintext               | `ingest/ia_croker.py`               |
| Kickham, *Knocknagow* (1879)                      | Gutenberg #44645                         | `ingest/gutenberg_kickham.py`       |

CORIECOR and the RIA Corpas Stairiúil are **not** automated — they require researcher contact / paid CD-ROM. Deferred: listed in README as "future sources" once baseline model is shipped.

## Data curation

- **`curate/dialogue_extractor.py`** — regex around `"…"`, `'…'`, and em-dash dialogue (Joyce/Griffin convention). Speaker attribution via verb-of-saying pattern (said/replied/cried/answered/muttered/whispered/roared/returned).
- **`curate/feature_tagger.py`** — rule-based tags mapped directly from the grammar table in the research doc (lines 111–146). Regexes for after-perfect (`\bafter\s+\w+ing\b`), habitual `do be`, cleft `'tis\s+\w+ing`, existential `in it`, detrimental `on (me|him|her|us|ye|them)`, emphatic reduplication, and a vocab-list lookup for discourse markers (wisha/musha/arrah/yerra/wirra).
- **`curate/joyce_pairs.py`** — Joyce often provides dialect→standard paraphrase (`"X" — i.e. "Y"`). Captured as paired examples.
- **`curate/class_assigner.py`** — YAML lookup `speaker_class.yaml` for known characters (Danny Mann → cottier, Hardress Cregan → gentry, Father Connell → priest). Heuristic fallback: heavy phonetic spelling → cottier; clean orthography → middling/gentry.
- **`curate/dedupe.py`** — `datasketch` MinHashLSH at paragraph level (Jaccard 0.85).
- **Volume target:** 80–120k dialogue spans (~600–800k tokens) post-dedup. If <30k, escalate to CORIECOR outreach before training.

## Instruction-pair construction

JSONL schema in `data/processed/{train,val,test}.jsonl`:

```json
{
  "system": "You are an NPC in 1820s rural County Roscommon, Ireland. Speak in period-accurate Hiberno-English with Irish substrate grammar. Social class: cottier. Lean on these features when natural: after-perfect, do-be habitual, cleft sentences. Discourse markers allowed: wisha, musha, arrah.",
  "user": "A neighbour asks if you've seen the priest today.",
  "assistant": "Wisha, I am after seeing him below at the chapel, so I am — 'tis confessions he was hearing.",
  "meta": {"class": "cottier", "tags": ["after-perfect","discourse-marker:wisha","emphatic-reduplication"], "source": "handwritten"}
}
```

- **System-prompt template** lives once in `src/parish_train/build/instruction_pairs.py::build_system_prompt()` and is reused verbatim at inference time inside the Ollama Modelfile — single source of truth.
- **Classes:** `{cottier, small_farmer, middling_farmer, gentry, priest, schoolmaster}` (matches the research doc line 241).
- **Mix (by row count):** 70% literary-extracted, 25% Joyce dialect↔standard paraphrase, 5% hand-written anchor. Anchor rows carry `sample_weight=3.0` at training time to dominate stylistic signal.
- **Anchor set (~500 rows, COMMITTED at `data/handwritten/anchor.jsonl`):** covers 6 classes × 9 grammar features × the core discourse markers. Authored by hand using the feature table in the research doc as the spec.
- **Split:** 90/5/5 stratified on `(class, primary_tag)` via scikit-learn `StratifiedShuffleSplit`. Anchor rows are split identically so all classes × features appear in val and test.

## Training stack

**Library: axolotl** (`pip install axolotl[flash-attn]`) — declarative YAML, first-class QLoRA + Gemma chat-template support, works on both ROCm and CUDA so the same config file drives both training hosts.

`configs/qlora_gemma4_9b.yaml`:

- `base_model: google/gemma-4-9b-it`
- `adapter: qlora`, `load_in_4bit: true`, `bnb_4bit_quant_type: nf4`, `bnb_4bit_compute_dtype: bfloat16`
- `lora_r: 16`, `lora_alpha: 32`, `lora_dropout: 0.05`
- `lora_target_modules: [q_proj, k_proj, v_proj, o_proj, gate_proj, up_proj, down_proj]`
- `learning_rate: 2e-4`, `lr_scheduler: cosine`, `warmup_ratio: 0.03`
- `num_epochs: 3`, `optimizer: paged_adamw_8bit`, `gradient_checkpointing: true`
- `chat_template: gemma`, `train_on_inputs: false` (mask system+user, train on assistant only)
- Sequence length + batch size differ per host (overridden by launcher):
  - **Local (RX 9070, 16 GB):** `sequence_len: 1536`, `micro_batch_size: 1`, `gradient_accumulation_steps: 16`
  - **RunPod (A100-80GB):** `sequence_len: 4096`, `micro_batch_size: 4`, `gradient_accumulation_steps: 4`

## Hardware fit check

Rough VRAM at NF4 QLoRA on the 9070, seq 1536, mb 1, gradient-checkpointed:

| Component                                   | Est.      |
|---------------------------------------------|-----------|
| Base weights (9B × 0.5 B/param NF4)         | ~4.6 GB   |
| LoRA adapters + grads (r=16 × 7 modules)    | ~0.4 GB   |
| Paged 8-bit optimizer state                 | ~0.6 GB   |
| Activations (seq 1536, bs 1, ckpt)          | ~5–7 GB   |
| Kernels + fragmentation + ROCm bnb overhead | ~1.5 GB   |
| **Total**                                   | **~12–14 GB** |

Fits 16 GB but tight. Seq 2048 overflows. **Biggest risk:** `bitsandbytes-rocm` wheel availability for RDNA4 / ROCm 6.x — `scripts/run_local.sh` pre-flights this with `python -c "import bitsandbytes; print(bitsandbytes.__version__)"` and bails with a clear message pointing to `run_runpod.sh` if it fails.

RunPod: A100-80GB at ~$1.89/h × ~6 h ≈ $12 for a clean run at seq 4096. Cheap enough to be the default when iterating on data or hyperparameters.

## Evaluation

- **Held-out scenario set** (`eval/held_out_scenarios.py`): 60 hand-written situations × 5 classes = 300 prompts, distinct from `anchor.jsonl` and never seen at training.
- **Automated rubric** (`eval/rubric.py`): per generation, counts feature occurrences per 100 tokens for after-perfect, habitual `do be`, cleft `'tis…`, existential `in it`, detrimental `on me`, discourse markers, echo-verb answers instead of yes/no, emphatic reduplication. Score = weighted sum calibrated against the anchor set's mean. Plus an **anachronism block-list** (ok, okay, hi, hey, guys, awesome, cool) — any hit fails the example.
- **Social-register check:** cottier outputs ≥1 phonetic spelling / 50 tokens; gentry outputs ≤0.1 / 50 tokens.
- **`/prove rundale-dialect`** (CLAUDE.md rule 4 — gameplay proof is required): new harness script `mods/rundale/scripts/prove_rundale_dialect.toml` switches provider to `gemma4-rundale:9b`, walks into a cottage, speaks with a cottier and a priest, and asserts the rubric passes on the JSON output.
- **Manual A/B** (`eval/ab_compare.py`): same 30 prompts to base `gemma4:9b-it` and candidate `gemma4-rundale:9b`, two-column markdown at `eval/reports/ab_<date>.md` for human review. Run before merging.

**Success bar to merge:** rubric ≥1 substrate feature / 30 tokens for cottier class, ≤0.05 anachronism rate across all classes, and a green `/prove rundale-dialect`.

## Packaging for Ollama

1. `package/merge_lora.py` — load base in fp16, `PeftModel.from_pretrained`, `merge_and_unload()`, save to `models/merged-fp16/`.
2. `package/to_gguf.sh` — clones `llama.cpp` into `training/vendor/llama.cpp` (gitignored), runs `convert_hf_to_gguf.py models/merged-fp16 --outfile models/gemma4-rundale-f16.gguf`, then `llama-quantize models/gemma4-rundale-f16.gguf models/gemma4-rundale-q4_K_M.gguf q4_K_M`.
3. `configs/modelfile.gemma4-rundale`:
   ```
   FROM ./models/gemma4-rundale-q4_K_M.gguf
   TEMPLATE """<start_of_turn>user
   {{ .System }}
   {{ .Prompt }}<end_of_turn>
   <start_of_turn>model
   """
   PARAMETER temperature 0.85
   PARAMETER top_p 0.9
   PARAMETER repeat_penalty 1.08
   PARAMETER stop "<end_of_turn>"
   PARAMETER stop "<start_of_turn>"
   SYSTEM """You are an NPC in 1820s rural County Roscommon, Ireland. Speak in period-accurate Hiberno-English with Irish substrate grammar."""
   ```
4. `ollama create gemma4-rundale:9b -f training/configs/modelfile.gemma4-rundale`.

## Parish wiring

- **`parish.example.toml`** — append a commented opt-in example under the existing provider block:
  ```toml
  # [provider.dialogue]
  # name = "ollama"
  # base_url = "http://localhost:11434"
  # model = "gemma4-rundale:9b"   # see training/README.md to build this
  ```
- **Feature flag** (per CLAUDE.md rule 6): gate the Rundale-specific system-prompt injection behind `config.flags.is_enabled("rundale-dialect-model")`. Flag default-off at merge; flipped default-on in a follow-up PR once the model is built and eval has passed. The flag controls only the dialect system prompt — if off, the engine uses the generic Dialogue prompt regardless of which model is wired up, so users who point `[provider.dialogue]` at stock `gemma4:9b` are unaffected.
- **Doc follow-ups** (same PR as the feature flag):
  - [`docs/design/inference-pipeline.md`](inference-pipeline.md) — add `gemma4-rundale:9b` as an optional Dialogue pick under "Recommended Models (April 2026)".
  - [`docs/adr/005-ollama-local-inference.md`](../adr/005-ollama-local-inference.md) — append a "Specialist models" subsection pointing at the new ADR.
  - [`docs/research/Irish-English-1820s-resources.md`](../research/Irish-English-1820s-resources.md) — append outcome notes (data volumes, rubric scores, A/B findings) under the existing "For Fine-Tuning" section.
  - **New ADR** `docs/adr/0NN-rundale-dialect-model.md` — documents the QLoRA decision, dataset provenance, eval results, and serving path.

## Critical files to create / modify

**Create:**
- `training/pyproject.toml`, `training/README.md`, `training/.gitignore`
- `training/configs/qlora_gemma4_9b.yaml`
- `training/configs/modelfile.gemma4-rundale`
- `training/src/parish_train/ingest/{gutenberg_joyce,ia_griffin,gutenberg_carleton,ia_croker,gutenberg_kickham,common}.py`
- `training/src/parish_train/curate/{dialogue_extractor,feature_tagger,joyce_pairs,class_assigner,dedupe}.py`
- `training/src/parish_train/build/{instruction_pairs,handwritten_anchor,split}.py`
- `training/src/parish_train/eval/{rubric,held_out_scenarios,ab_compare}.py`
- `training/src/parish_train/package/{merge_lora,build_modelfile}.py` + `to_gguf.sh`
- `training/scripts/run_local.sh`, `training/scripts/run_runpod.sh`
- `training/data/handwritten/anchor.jsonl` (~500 rows, hand-authored)
- `training/data/LICENSES.md`
- `mods/rundale/scripts/prove_rundale_dialect.toml` (new `/prove` harness script)
- `docs/adr/0NN-rundale-dialect-model.md`

**Modify:**
- `parish.example.toml` — add commented `[provider.dialogue]` example
- `docs/design/inference-pipeline.md` — add `gemma4-rundale:9b` to Dialogue recommendations
- `docs/adr/005-ollama-local-inference.md` — Specialist models subsection
- `docs/research/Irish-English-1820s-resources.md` — append outcome notes
- `parish-core` (wherever Dialogue system-prompt is assembled) — add the `rundale-dialect-model` flag check. The exact module will be identified by grepping for the current dialogue system-prompt assembly site during implementation; no speculative changes made here.

## Verification — end-to-end

```sh
# 0. one-time setup
cd /home/user/Parish/training
uv sync

# 1. ingest + curate + build
uv run python -m parish_train.ingest.gutenberg_joyce
uv run python -m parish_train.ingest.ia_griffin
uv run python -m parish_train.ingest.gutenberg_carleton
uv run python -m parish_train.ingest.ia_croker
uv run python -m parish_train.ingest.gutenberg_kickham
uv run python -m parish_train.curate.dialogue_extractor
uv run python -m parish_train.curate.feature_tagger
uv run python -m parish_train.curate.dedupe
uv run python -m parish_train.build.instruction_pairs
uv run python -m parish_train.build.split

# 2. train — try local first, fall back to RunPod if ROCm bnb missing or OOM
bash scripts/run_local.sh                 # pre-flights bitsandbytes, then axolotl
# OR
bash scripts/run_runpod.sh                # uploads data + config, runs on A100

# 3. eval
uv run python -m parish_train.eval.rubric --adapter models/qlora-out/
uv run python -m parish_train.eval.ab_compare --base gemma4:9b-it --candidate models/qlora-out/

# 4. package for Ollama
uv run python -m parish_train.package.merge_lora
bash src/parish_train/package/to_gguf.sh
ollama create gemma4-rundale:9b -f configs/modelfile.gemma4-rundale
ollama run gemma4-rundale:9b "What's after happening below?"   # smoke test

# 5. wire into Parish + prove
cd /home/user/Parish
cp parish.example.toml parish.toml        # uncomment [provider.dialogue] block
just check                                # fmt + clippy + Rust tests (unchanged)
# run the new gameplay proof per CLAUDE.md rule 4
/prove rundale-dialect
```

**Green bar to merge:**
1. `just check` passes.
2. `eval/rubric.py` reports ≥1 substrate feature / 30 tokens on the cottier slice and ≤0.05 anachronism rate overall.
3. `/prove rundale-dialect` passes.
4. Manual A/B report shows the fine-tune is clearly more period-appropriate than stock `gemma4:9b-it` on ≥70% of 30 paired prompts.
