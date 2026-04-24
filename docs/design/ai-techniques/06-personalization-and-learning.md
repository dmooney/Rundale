# Personalisation & Online Learning

**Target crate:** new `crates/parish-preferences/`, integrations in
`crates/parish-npc/` and `crates/parish-inference/`. Offline training lives in
`training/` (mirrors `gemma4-rundale-training-plan.md`).

## Problem

Every player gets the same Rundale voice. A player who enjoys dry, long
exchanges gets the same register as one who types terse verbs. NPCs neither
mirror the player's idiolect nor improve from feedback. We also have no
pipeline to *learn* from the thousands of good conversations players will
generate.

## SOTA techniques

### 1. In-context player modelling

Maintain a small JSON "player profile" updated by a Tier 3-adjacent job:

- Preferred register (plain / ornate / terse).
- Interests (names, trades, places they return to).
- Tolerated pace (short dialogue vs long).
- Language hints (Irish loanwords welcome or not).

Inject into the Tier 1 system prompt as an additional stanza. Cheap, entirely
local, no gradient updates required. Update via a tiny LLM pass on recent
transcripts.

### 2. Bandit-style dialogue variant selection

When multiple candidate responses are generated (doc 03 rejection sampling),
record which the player engaged with (reply length, follow-up, thumbs).
Treat as a contextual bandit:

- Arms: response styles (concise / lyrical / blunt / teasing).
- Context: player profile + NPC persona.
- Reward: engagement signal.

LinUCB or Thompson sampling on a small feature vector. No GPU, no fine-tune.

### 3. DPO / KTO from player feedback

Once a feedback corpus exists:

- **DPO** (Rafailov 2023): preference pairs → direct policy optimisation.
- **KTO** (Ethayarajh 2024): single-sided signals (thumbs up / down), easier
  to collect in-game.

Apply as a LoRA on top of the base Tier 1 model. Ship per-player or
per-cohort. Gated behind `--enable-feedback-learning` (opt-in, privacy
disclosure).

### 3a. Distillation from cloud into local via emoji-sentiment filter

`crates/parish-npc/src/reactions.rs` already logs player emoji reactions
per turn. That log is a free preference signal:

1. **Harvest:** for every Tier 1 turn routed to cloud (Claude Opus /
   Sonnet), persist `(system prompt, context, response, emoji reactions,
   follow-up length)` to `training/traces/`.
2. **Filter:** keep turns whose reactions skew positive
   (emoji sentiment score > τ) *and* whose follow-up engagement is
   non-trivial. Discard turns with negative or no reaction.
3. **Fine-tune:** LoRA adapter over a local base (Qwen 2.5 7B, Mistral
   Nemo, Gemma 2 9B) on the filtered set. Merge or hot-swap at inference.
4. **Evaluate:** judge harness (doc 09) gates promotion — the local model
   must match or beat cloud on the golden corpus before it routes.

End state: Tier 1 runs local-first for the common case at near-cloud
quality, cloud is reserved for genuinely hard turns or players who opt in.
Shares tooling with `docs/design/gemma4-rundale-training-plan.md`.

Effort: ~3 engineer-weeks + GPU time. Only worth starting after doc 02
grammar and doc 04 tool-call schemas have stabilised — otherwise the
distilled model learns an unstable output contract.

### 4. Self-play rollouts for evaluation

Use two instances of the Tier 1 model playing player/NPC roles against each
other on a curated scenario set. Score transcripts with the doc 03 judge.
Any new candidate model or LoRA must beat the baseline on win-rate before
shipping.

### 5. Persona steering vectors (activation engineering)

Representation-engineering techniques (RepE, ITI) let us *steer* base models
with a single activation vector — e.g. a "1820s Connacht dialect" vector
learned from a few dozen examples. Cheaper than a LoRA and swappable at
request time. Early research but viable on llama.cpp with patching.

### 6. Federated / on-device fine-tuning

Tauri build runs on the player's machine. Periodic nightly jobs can fine-tune
a small adapter against that player's corpus, without ever sending data off.
Privacy-preserving, ethically clean; web build won't get this.

### 7. NPC-specific memory + style convergence

Memory system (doc 01) feeds style too: after N conversations with the
player, each NPC has a personalised retrieval-augmented style. The NPC
"learns" the player without any weight updates — emergent familiarity.

## Minimal first cut

1. Add `crates/parish-preferences` with a `PlayerProfile` struct, JSON
   persisted next to saves.
2. Nightly (in-game dawn) tick that re-runs a short LLM profile update over
   the last day's transcripts.
3. Inject profile block into `tier1_system.txt` template.
4. Log dialogue thumbs in `parish-types::conversation`; use only for
   analytics initially. Fine-tuning is a later phase.

## Risks

- Privacy: any learned artifact must stay local by default. Explicit opt-in
  for uploading preference pairs.
- Overfit to a single player: style collapses to mirror-the-player. Regularise
  with an anchor toward NPC canonical voice.
- Cohort bias from DPO: small datasets skew fast. Require N ≥ 500 pairs before
  shipping a LoRA update.

## Papers / references

- Rafailov et al., *Direct Preference Optimization* (2023).
- Ethayarajh et al., *KTO: Model Alignment as Prospect-Theoretic Optimization* (2024).
- Li & Liang, *Prefix-Tuning* (2021) — relevant to cheap per-player adapters.
- Zou et al., *Representation Engineering: A Top-Down Approach to AI Transparency* (2023).
- Li et al., *Inference-Time Intervention* (ITI, 2023).
