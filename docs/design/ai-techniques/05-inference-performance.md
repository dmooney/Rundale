# Inference Performance

**Target crate:** `crates/parish-inference/` (scheduler, provider clients),
optional native llama.cpp backend.

## Problem

Ollama calls sit on the critical path of Tier 1 dialogue. On a laptop, a 9B
model answers in ~1.5–3s uncached. Tier 2 ticks fire every 5 game-minutes and
can easily pile up. The Interactive lane is fine; the Background lane starves
on machines with a single GPU.

## SOTA techniques

### 1. Prompt / KV cache reuse

Every Tier 1 turn rebuilds a prompt that is mostly identical: world rules,
scene description, NPC persona, long-term memory summary, *then* the new turn.
llama.cpp and vLLM both support **prefix caching** — reuse the KV tensors for
the shared prefix, pay only for the changed tail.

- **Local (llama.cpp/Ollama):** enable `cache_prompt: true`, structure prompts
  so the dynamic suffix is genuinely last.
- **Cloud:** Anthropic offers explicit `cache_control` breakpoints; OpenAI
  caches automatically on ≥1024-token prefixes. Audit our prompt builder
  (`crates/parish-npc/src/lib.rs` system/context builders) so the static block
  comes first and ends on a stable boundary.

Expected: 30–70% latency reduction on Tier 1 continuations.

### 2. Speculative decoding

Pair the 9B target model with a 1B–3B draft. Draft produces candidate tokens,
target verifies in parallel. Works especially well for structured JSON tails
(doc 02) where draft accuracy is high.

- **llama.cpp:** `--draft-model` flag, zero-code.
- **Ollama:** exposed via the underlying llama.cpp runner; wire through.
- **Cloud:** some providers expose this server-side already; no-op for us.

Expected: 1.7–2.3× throughput at equal quality.

### 3. Continuous batching (vLLM-style)

For Tier 3 daily sim we issue 50+ NPC summary calls back-to-back. Today
they're serialised. With a vLLM backend or llama.cpp `--parallel N`, tokens
from all requests interleave in the same forward pass.

Trade-off: extra complexity of running vLLM alongside Ollama. Viable path is
a dedicated "batch" provider (`parish-inference::provider::Vllm`) used only
for Tier 3.

### 4. Quantisation ladder

We run Q4_K_M today (typical Ollama). Evaluate:

- Q5_K_M for Tier 1 (quality ↑, VRAM ↑ ~15%).
- Q3_K_S for Tier 3 batch sim (quality ↓ tolerable, throughput ↑ ~30%).
- FP16 draft model for speculative decoding (tiny anyway).

Ship per-tier model ids already supported by ADR-017 per-category routing.

### 5. LoRA adapters per NPC archetype

Instead of one persona prompt per NPC, train small LoRA adapters per
*archetype* (landlord, priest, fisherman, publican). Swap adapters per
request — llama.cpp and vLLM both support hot-swap. Smaller prompt, sharper
voice, and a natural place to encode period dialect.

See `docs/design/gemma4-rundale-training-plan.md` — this slots in alongside.

### 6. Smarter priority lanes

Current lanes (16/32/64) are capacity-based. Upgrade to:

- **Deadline-aware:** Tier 1 has a player-perceived deadline (<2s), Tier 2
  has a game-time deadline (<5 min real), Tier 3 has an hours-long window.
- **Preemption:** Tier 1 arrivals preempt in-flight Tier 2 tokens on the same
  GPU (llama.cpp supports request cancellation).

### 7. Edge offload / tiered routing

Let the scheduler send the "easy" subset of Tier 3 ticks (NPCs with minimal
events) to an even smaller 1B model, reserving 9B for NPCs with rich
activity. Classify via a heuristic or a tiny model.

### 8. Utility lane — small-LM for non-dialogue tasks

A large share of our current LLM calls are *not prose*: intent
classification, mood extraction, gossip distortion (doc 07), anachronism
flagging (doc 03), memory-importance scoring, knowledge-graph triple
extraction (doc 10), inner-monologue (doc 03). All run well on 0.5–3B local
models (Gemma 3 270M, Phi-4-mini, Qwen 2.5 1.5B, Llama 3.2 1B).

Add a `Utility` category alongside the existing `Dialogue / Simulation /
Intent / Reaction` routing slots (ADR-017). Benefits:

- Keeps the 9B busy on prose only; Utility jobs never queue behind a Tier 1.
- Cuts cloud spend significantly — Utility runs local-first by default.
- Unblocks several downstream doc entries (3.3 judge, 6.2 inner-monologue,
  10 extraction, 07 rumour mutation) that all want a cheap per-turn pass.

Foundational work — ship before anything that adds a second LLM call on the
Tier 1 path.

### 9. Long-context world-state packing

Some providers now ship 200K–1M context (Claude 3.7, Gemini 2.5, Llama 4
Scout local). For the cloud Tier 1 lane (ADR-013), build an optional
`build_longcontext_prompt` that packs a substantial slice of village state:

- Full NPC roster with current mood / state.
- Recent Tier 2 event feed for the quarter.
- All directly referenced location descriptions.
- The player's last N turns unabridged.

Coherence climbs (NPCs stop forgetting yesterday) and prompt-builder code
simplifies because we stop hand-curating context. Anthropic prompt caching
(already present in our client) makes the cost of a stable long prefix
near-free on subsequent turns.

Best paired with doc 01 semantic memory and doc 10 knowledge graph: use
retrieval to *prioritise* what goes in the long context, not to *replace*
it.

## Minimal first cut

1. Audit `parish-npc::prompt_*` builders: move all static blocks to the top,
   end static content on a known token boundary.
2. Enable `cache_prompt`/`cache_control` everywhere; add a prompt-hash debug
   field in `/debug` UI to verify cache hit rate.
3. Add a `draft_model` config key to per-category routing; test Gemma 2B as
   draft for 9B target.
4. Add feature flag `batch-tier3-vllm` and implement a vLLM provider variant.

## Risks

- KV cache invalidation bugs produce stale context; tests must diff prompts
  against the cache key.
- Speculative decoding with grammar masking interacts subtly — verify with
  the doc 02 grammar path turned on.
- vLLM is Linux/CUDA-heavy; mode parity requires a fallback for macOS
  (Tauri) and web deploy.

## Papers / references

- Leviathan et al., *Fast Inference from Transformers via Speculative Decoding* (2022).
- Chen et al., *Accelerating LLM Decoding with Speculative Sampling* (2023).
- Kwon et al., *Efficient Memory Management for LLM Serving with PagedAttention* (vLLM, 2023).
- Hu et al., *LoRA: Low-Rank Adaptation of LLMs* (2021).
- Anthropic, *Prompt Caching* docs (2024).
