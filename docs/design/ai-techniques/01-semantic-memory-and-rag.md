# Semantic Memory & Retrieval-Augmented NPCs

**Target crate:** `crates/parish-npc/` (extend `memory.rs`), optional new
`crates/parish-embeddings/`.

## Problem

`LongTermMemory::recall` (`crates/parish-npc/src/memory.rs`) scores entries by
keyword overlap. A memory stored as *"Máire's cow took sick at the fair"* is
invisible to a query about *"cattle illness in Tuam"* — the words don't match
even though the meaning does. NPCs feel amnesiac.

## SOTA techniques

### 1. Dense retrieval with small embedding models

- **Models:** `nomic-embed-text-v1.5` (768d, Ollama-hostable),
  `mxbai-embed-large` (Ollama-native, 1024d, strong on retrieval),
  `bge-small-en` (384d), `gte-small`. All run locally in ~50ms per batch
  on CPU.
- **Storage (picks):** SQLite `vec0` / `sqlite-vss` for a single dependency
  tier (we already use SQLite, ADR-003). For higher-recall ANN, `hnsw_rs`
  (pure Rust) or embedded Qdrant; both run in-process, no extra service.
- **Schema:** one vector row per memory entry, joined to the existing
  `LongTermEntry` by id.

### 2. Hybrid retrieval (BM25 + dense)

Keyword match is still useful for proper nouns ("Máire", "Ballygar"). Combine
with reciprocal-rank fusion — it consistently outperforms either alone on noisy
domains. Keep current keyword store as the lexical arm.

### 3. Reflection / consolidation (Generative Agents, Park et al. 2023)

Periodically (say, at dawn in-game) run a Tier 3 pass per NPC that:

- Retrieves the top-k recent memories.
- Asks the LLM to produce 1–3 higher-order *reflections*
  ("I think the landlord is afraid of me").
- Stores them as first-class memories with boosted importance.

This compresses episodic detail into semantic beliefs — directly fixes the
"NPCs don't seem to learn" complaint.

### 4. MemGPT-style paging

For Tier 1, the prompt context is small. Treat long-term memory as disk and
short-term as RAM:

- LLM emits a tool call `recall(query)` if it senses missing context.
- Inference pipeline resolves the call, injects top-k hits, re-enters the
  conversation.
- Naturally layers on top of the function-calling work in `04-agent-planning`.

### 5. Historical-corpus RAG (style + period grounding)

Reuse the same embedding infra to index *external* period sources, not just
NPC memory:

- Public-domain 1820-era Irish material: newspapers (Galway Advertiser,
  Freeman's Journal), letters, traveller accounts (Arthur Young, Edward
  Wakefield), parliamentary reports.
- Stored under `mods/rundale/corpus/` with per-document license metadata.
- A small indexing CLI chunks, embeds, and writes to a separate `corpus_vec`
  table (no mingling with NPC memory).
- Tier 1 prompt builder retrieves 1–2 snippets by topic of conversation and
  injects them as a quoted "period voice" stanza, replacing the currently
  frozen cultural-guidelines paragraph.

Payoff: dialogue inherits *actual* period cadence and vocabulary instead of
the model's pastiche. Compounds with doc 06's style-vector work.

### 6. Episodic vs semantic split

Split `LongTermMemory` into:

- **Episodic:** timestamped events (keep current shape).
- **Semantic:** distilled facts / beliefs (output of reflection).
- **Procedural:** skills / routines (feeds Tier 4 schedule generation).

Retrieval can then weight by kind depending on the query
(procedural weighted higher at work-time, episodic at the pub).

## Minimal first cut

1. Add `crates/parish-embeddings` with an `Embedder` trait
   (Ollama + OpenAI implementations, mirroring `parish-inference`).
2. Extend `LongTermEntry` with an optional `embedding: Vec<f32>`.
3. Persist vectors in a new `long_term_vec` SQLite table.
4. Replace `recall` with hybrid retrieval; gate behind flag
   `semantic-memory` (default off until validated).
5. Add a nightly reflection Tier 3 job that promotes clusters to semantic
   memories.

## Risks

- **Cost:** embedding every short-term event doubles Ollama calls. Mitigate by
  only embedding at promotion time (importance ≥ 0.5).
- **Drift:** embeddings tie memory layout to a specific model. Store
  `model_id` per vector and re-embed lazily on model change.
- **Mode parity:** web / Tauri / CLI must all carry the same `vec0` build.

## Papers / references

- Park et al., *Generative Agents: Interactive Simulacra of Human Behavior* (2023).
- Packer et al., *MemGPT: Towards LLMs as Operating Systems* (2023).
- Izacard & Grave, *Atlas* — retrieval-augmented few-shot learning (2022).
- Gao et al., *Retrieval-Augmented Generation for LLMs: A Survey* (2024).
