# Dialogue Quality Loops

**Target crate:** `crates/parish-inference/` (critic lane), `crates/parish-npc/`
(post-processing), new `testing/judges/`.

## Problem

A Tier 1 reply can be grammatically fine yet wrong for Rundale: wrong dialect,
wrong century, out-of-character knowledge, or violates an NPC's mood.
`crates/parish-npc/src/anachronism.rs` catches word-level leaks but misses
_semantic_ anachronism ("sure, I'll check my calendar on Tuesday").

## SOTA techniques

### 1. Self-Refine / Reflexion (single-model critique)

1. Generate draft response with Tier 1 model.
2. Prompt the same model with a critic persona
   ("You are a dialogue coach. List any anachronism, OOC knowledge, or
   tone mismatch.").
3. If critique is non-empty, regenerate conditioned on the critique.

Cost: 2–3× tokens on flagged turns only. Fits the **Interactive** lane if the
critic runs on a smaller sidekick model (the 3B we already use for intent).

### 2. LLM-as-judge + rejection sampling

- Generate N=3 candidates in parallel.
- Rank with a judge prompt using rubric: _dialect_, _persona fit_, _mood fit_,
  _factual consistency with memory_.
- Return the top-ranked; discard the rest but log them for preference data
  (feeds `06-personalization-and-learning`).

Works great offline for evaluation; at runtime only affordable for hero NPCs
or marked "key conversations".

### 3. Constitutional rules as prompts

Codify period constraints as a short list of principles (no telephones, no
post-1820 vocabulary, class register matters). Apply as a system-level guard
pass — cheap, catches regressions.

### 4. Style rubric with retrieval

Maintain a small corpus of "gold" 1820s-authentic lines (curated from
`mods/rundale/` dialogue reviews). Retrieve the 3 nearest examples by
embedding (reuse `01-semantic-memory`) and inject as few-shot exemplars. This
is retrieval-augmented _style_, not retrieval-augmented _facts_.

### 5. Uncertainty-aware abstention

Current NPCs confabulate when asked something they can't know. Use token-level
logprobs:

- If the model's answer has average logprob below a threshold on named
  entities, rewrite to hedge ("I couldn't say, sir").
- Requires logprobs from the provider — Ollama exposes, most cloud routes do.

### 6. Inner-monologue / think-then-speak

Foreshadowed already by the `internal_thought` field Tier 1 emits today
(ADR-008). Split the turn into two calls:

1. **Think** — a cheap utility-model pass on the NPC's private scratchpad:
   goals, secrets to withhold, register choice, what they _won't_ say.
2. **Speak** — the main Tier 1 generation, conditioned on the think-trace
   but instructed not to quote it verbatim.

Meaningful payoff: NPCs can plan to lie, to deflect, or to hold something
back, and the deception is consistent because the think-trace is stored in
short-term memory. Combines cleanly with doc 10 (the think pass can query
the knowledge graph to decide what is safe to say).

Cost: one extra small-model call per turn. Budget it into the utility lane
from doc 05 (~150–300 ms on a 1–3B model).

### 7. Critic in the pipeline, not the client

Add a `CriticJob` variant to `parish-inference::job`. It runs on the Background
lane with a shared KV cache off the original Tier 1 context (see
`05-inference-performance`). The draft is shown immediately; if the critic
flags, a _correction_ bubble replaces the turn before the player can respond.

### 8. Inference-time rejection sampler (Stage 3 of the Rundale dialect model)

Implements §7's Background-lane critic pattern with the **three-axis
local-only** judge stack from the [Gemma 4 Rundale training plan](../../plans/gemma4-rundale-training-plan.md#stage-3-—-inference-time-rejection-sampler):
deterministic anachronism wordlist (Webster 1828 ∪ Joyce 1910 ∪ Wright EDD
1898–1905), Talkie-1930-13B-IT (q4) log-likelihood under a fixed
Roscommon-1820s system prompt, and the ~250M Rundale dialect-oracle
log-likelihood. **DeepSeek V4-pro is dropped at serve time** — API roundtrip
× K candidates blows the latency budget; reserved for offline DPO and the
`llm-quality-evals` nightly regression sensor.

**Ollama serialises generation per instance**, so an inline best-of-K loop is
not viable (K=4 × ~400 ms = ~1.6 s of generation alone). The Stage-3 wrapper
therefore ships the Tier 1 draft immediately (≤ 600 ms unchanged), dispatches
K-1 alternates + scoring on the Background lane, Borda-aggregates draft +
alternates, and **silently replaces the bubble** if the draft loses. Critic
wall-clock cap **1500 ms**; abandon past that and keep the draft as-shipped.

One set of scorers, two evaluation modes selected by
`judge_combined.py --mode {dpo|serve}`. DPO uses N=8 (no latency cap, more
preference signal); serve uses K=4 (fits the 1500 ms critic budget given
Ollama's serial generation).

Gated by `config.flags.is_enabled("inference-rejection-sampler")` per CLAUDE.md
non-negotiable rule 6. This flag is **independent of** `rundale-dialect-model`
(which gates the dialect system prompt + model selection); per rule 6 both
flags ship **default-on** in the same PR as the artifact they gate. The flag
exists to allow disabling, not to stage rollout — eval gating belongs in CI,
not in flag-flip choreography.

The sampler wraps the existing JSON serving contract without modifying it
(`NpcJsonResponse` at `parish/crates/parish-npc/src/lib.rs:216`,
`extract_dialogue_from_partial_json` at `parish/crates/parish-types/src/ids.rs:229`)
— same schema applied to all K candidates.

## Minimal first cut

1. Offline only: build `testing/judges/tier1_judge.py` with a 5-criterion
   rubric; run nightly over sampled conversations from `parish-types/conversation.rs`
   logs; publish scorecard.
2. Gate `self-refine-tier1` flag; on flag, add one critic pass per turn using
   the 3B intent model re-prompted as a coach.
3. Expose judge scores in `/debug` UI to speed iteration.

## Risks

- Cascading latency. Budget: draft ≤ 600ms, critic ≤ 300ms; hard cancel past
  900ms and ship the draft.
- Over-correction produces bland output. Measure by judge rubric _before_
  shipping self-refine to players.
- Judge model bias (favouring verbose responses). Use pairwise preference, not
  absolute scoring, where possible.

## Papers / references

- Madaan et al., _Self-Refine: Iterative Refinement with Self-Feedback_ (2023).
- Shinn et al., _Reflexion: Language Agents with Verbal Reinforcement Learning_ (2023).
- Zheng et al., _Judging LLM-as-a-Judge with MT-Bench and Chatbot Arena_ (2023).
- Bai et al., _Constitutional AI_ (Anthropic, 2022).
- Kadavath et al., _Language Models (Mostly) Know What They Know_ (2022).
