# SOTA AI Techniques — Brainstorm

A menu of state-of-the-art AI/ML techniques we could incorporate into Rundale.
Each entry is a short design note pitched against what the engine already has
(see `docs/adr/002-cognitive-lod-tiers.md`, `docs/adr/005-ollama-local-inference.md`,
`crates/parish-npc/`, `crates/parish-inference/`).

These are brainstorm notes, not committed plans. Each technique should graduate
to an ADR + feature flag (`crates/parish-config/src/flags.rs`) before shipping.

## Baseline (what we already have)

- 4-tier cognitive LOD (Tier 1 player dialogue → Tier 4 rules).
- Local Ollama + optional cloud routing per category (ADR-005, ADR-013, ADR-017).
- Priority-lane inference queue (`crates/parish-inference/src/lib.rs`).
- Keyword-based memory: 20-entry short-term ring + 50-entry long-term
  (`crates/parish-npc/src/memory.rs`).
- Structured JSON output with `---` separator (ADR-008).
- Anachronism / prompt-injection defense (`crates/parish-npc/src/anachronism.rs`,
  ADR-010).
- Hand-authored NPCs / world / schedules (`mods/rundale/`).

## Gap map (where SOTA would move the needle)

| Gap | Today | Opportunity |
| --- | --- | --- |
| Retrieval | Keyword overlap | Semantic embeddings + RAG |
| Memory consolidation | Importance threshold promotion | Reflection, summarisation agents |
| Truth gating | Prompts leak world facts | Symbolic knowledge graph per NPC (doc 10) |
| Dialogue reliability | Post-hoc JSON parse | Constrained decoding (grammar) |
| Dialogue quality | Single-shot generation | Self-refine, inner monologue, critic passes |
| NPC agency | Schedule + mood deltas | Tool-using agents; effect-producing v2 tools |
| Narrative pacing | Pure sandbox | AI director with archetype pool (doc 11) |
| Authoring throughput | Hand-crafted mods | LLM-assisted authoring with review (doc 12) |
| Local perf | Sequential Ollama calls | Prompt cache, speculative decoding, Utility lane |
| Voice | Hand-written prompts | LoRA-tuned period dialect model, emoji-filtered distillation |
| Player adaptation | Static persona prompt | Online player modelling |
| Social spread | Probabilistic rules | Graph diffusion + theory-of-mind + rumour mutation |
| Multimodal | Text only | TTS (+ `pronunciations.json`), ASR, diffusion portraits |
| Evaluation | Unit tests + /prove | LLM-as-judge, adversarial-player harness |

## Topic notes

1. [`01-semantic-memory-and-rag.md`](01-semantic-memory-and-rag.md) — embeddings,
   hybrid retrieval, reflection, MemGPT-style paging, historical-corpus RAG.
2. [`02-structured-generation.md`](02-structured-generation.md) — GBNF / JSON
   schema constrained decoding, grammar-guided output.
3. [`03-dialogue-quality-loops.md`](03-dialogue-quality-loops.md) — self-refine,
   reflexion, LLM-as-judge, inner-monologue think-then-speak, rejection sampling.
4. [`04-agent-planning-and-tools.md`](04-agent-planning-and-tools.md) — ReAct,
   read-only tools v1, mutating tools v2 ("I'll fetch the priest" dispatches
   Máire), tree-of-thought.
5. [`05-inference-performance.md`](05-inference-performance.md) — prompt / KV
   cache reuse, speculative decoding, continuous batching, LoRA adapters,
   Utility lane for small-LM tasks, long-context world packing.
6. [`06-personalization-and-learning.md`](06-personalization-and-learning.md) —
   player modelling, DPO/KTO from thumbs, emoji-sentiment distillation from
   cloud to local.
7. [`07-social-simulation.md`](07-social-simulation.md) — theory-of-mind beliefs,
   multi-agent debate for Tier 2, rumour mutation, graph diffusion for gossip.
8. [`08-multimodal.md`](08-multimodal.md) — Whisper ASR, TTS with per-NPC voice
   plus `pronunciations.json` hook, diffusion portraits and ambient art.
9. [`09-evaluation-and-safety.md`](09-evaluation-and-safety.md) — LLM-as-judge
   harness, red-team suite, adversarial-player fuzzing, calibrated abstention.
10. [`10-knowledge-graph-grounding.md`](10-knowledge-graph-grounding.md) —
    symbolic triple store with provenance and confidence; gates what NPCs can
    say about what.
11. [`11-drama-manager.md`](11-drama-manager.md) — daily AI director that
    injects narrative pressure from an authored archetype pool.
12. [`12-llm-assisted-authoring.md`](12-llm-assisted-authoring.md) — mod
    generation pipeline with author-in-the-loop review.

## Effort & dependencies (rough)

| Topic | Effort | Best after |
| --- | --- | --- |
| 01 semantic memory + historical RAG | ~1 week (+ corpus curation) | — |
| 02 structured generation | ~3 days | — |
| 03 quality loops (incl. inner monologue) | ~4 days + ~1 week | 05 Utility |
| 04 agent planning (v1 read-only) | ~1 week | 02 |
| 04 agent planning (v2 mutating) | +1 week | 04 v1, 07 |
| 05 inference perf (KV cache, spec decode) | ~1 day + audit | — |
| 05 Utility lane | ~4 days | — (foundational) |
| 05 long-context packing | ~1 week | 01 |
| 06 distillation from emoji-filtered traces | ~3 weeks + GPU | 02, 04 |
| 07 social sim (beliefs, rumour mutation) | ~1 week | 10 |
| 08 TTS with pronunciations | ~2 weeks | — |
| 09 eval harness | ~4 days | 02 |
| 10 knowledge graph | ~2 weeks | 01 |
| 11 drama manager | ~1.5 weeks | 02, 10 |
| 12 LLM-assisted authoring | ~2 weeks | 02, 09 |

## Prioritisation heuristic

Rank each technique by:

- **Player-visible impact** (does it change the feel of a conversation?).
- **Implementation cost in Parish** (does it fit the crate boundary?).
- **Local-first compatibility** (can it run under Ollama, or does it need cloud?).
- **Mode parity** (ADR rule: CLI / web / Tauri must agree).

High-impact + low-cost + local-first should ship first: semantic memory
(01), grammar-constrained generation (02), and the Utility lane (05) are
the obvious starting points, and they unblock almost everything else.

Close-second tier, once those three land:

- Read-only tools (04 v1) — unlocks director + knowledge-graph queries.
- Knowledge graph (10) — everything downstream (gossip, secrets, quests).
- AI director (11) — fixes the "nothing is happening" feel.

Distillation (06.3a) and authoring (12) are last: both depend on stable
schemas from 02 and 04 to avoid learning the wrong contract.
