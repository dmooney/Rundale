# ADR-021: Embedding-Based NPC Memory Retrieval

> Back to [ADR Index](README.md) | [Docs Index](../index.md)

## Status

Proposed (2026-04-26). No implementation yet.

## Context

NPCs in Rundale carry two memory stores today (see [docs/design/npc-system.md](../design/npc-system.md) and `crates/parish-npc/src/memory.rs`):

1. **Short-term** — a 20-entry ring buffer of recent observations and conversations.
2. **Long-term** — accumulating entries with no eviction beyond manual pruning.

When the engine constructs a Tier 1 prompt, it injects **all** long-term memories (or a recency-truncated slice) into the context. As an NPC's life progresses across many in-game days, this approach has predictable failures:

- **Context pressure.** Long-term memory grows without bound; eventually it exceeds the model's context window or becomes the dominant chunk of every prompt, crowding out the system prompt and scene context.
- **Recency bias.** Truncating to the most recent N entries throws away exactly the kind of memory that makes an NPC feel alive — "remember when you helped me with the harvest last spring" gets evicted long before "you said hello yesterday."
- **No relevance signal.** A player asking about the priest in St. Brigid's Church doesn't need every memory in the NPC's life — they need memories about the priest, the church, and the player's prior interactions on related topics.

The standard fix is **embedding-based retrieval**: store each memory as a vector, embed the prompt context, retrieve the top-K most relevant memories per turn. This works well in production agent systems but adds infrastructure: an embedding model, a vector store, an embedding step on every memory write, and a retrieval step on every prompt build.

## Decision

**Deferred.** Two prerequisites should land before we commit:

1. **A reliable lightweight embedding option.** The local-first commitment from [ADR-005 Ollama Local Inference](005-ollama-local-inference.md) means we need an embedding model that ships with Ollama (or equivalent local runtime) and produces useful 256–768 dim vectors at acceptable latency. Today, the candidate is `nomic-embed-text` or `mxbai-embed-large` via Ollama — the local-quality bar should be measured before we rely on it.
2. **Quality measurement.** Without [LLM quality evals](../plans/llm-quality-evals.md), we cannot tell whether retrieval *improves* dialogue quality vs. simply changing it. The eval suite should land first so the retrieval rollout can be evaluated.

When we revisit, the candidate decision is:

> **Long-term memory entries are stored both as plaintext (the existing `Memory` struct) and as an embedding vector. At Tier 1 prompt-build time, the engine embeds the scene context (player input + location + speakers) and selects the top-K (K ≈ 6) memories by cosine similarity, falling back to recency if the embedding store is unavailable. Short-term memory is unchanged. Vector storage piggy-backs on `parish-persistence`'s SQLite — likely [`sqlite-vss`](https://github.com/asg017/sqlite-vss) or an in-process kNN over a `BLOB` column.**

## Consequences

### If accepted (when we revisit)

**Easier:**

- NPCs can carry a long, plausible life-history without exploding the prompt.
- Scenes feel more grounded — the right memory surfaces at the right moment without prompt-engineering hacks.
- Eval suite can rubric on "did the relevant past event surface" with a held-out test corpus.

**Harder:**

- **New dependency surface.** Either a vector-search SQLite extension (`sqlite-vss` adds platform-specific binaries that complicate Tauri packaging) or an in-process kNN (works but slower above a few thousand memories per NPC).
- **Embedding-time cost.** Every memory write becomes a model call. Tier 4 → Tier 1 promotion already runs an inflate step; embedding gets bundled there.
- **Provider routing.** Embedding category needs to be added to the per-category provider config from [ADR-017](017-per-category-inference-providers.md). Cloud-vs-local choice for embeddings is its own knob.
- **Migration.** Existing save files have memories without vectors; either lazily embed on first read or backfill on load.
- **Eval guarantee.** A bad retrieval (irrelevant memory floated to top) can produce *worse* dialogue than no retrieval — the eval suite must catch this.

### If rejected

Stay with the recency-truncation approach. Accept that long-lived NPCs won't carry deep memory. Mitigations available without retrieval:

- Per-NPC manual memory curation (a human picks the "core" memories that always go in).
- LLM-assisted compaction — periodically summarize old memories into a single "biographical summary" entry.

## Alternatives Considered

### A. LLM-assisted summarization instead of retrieval

Periodically run a Tier 2/3 simulation step that compacts the bottom-N memories into a single summary entry. Cheaper infrastructure, no embeddings, no vector store. Loses fidelity — the summary becomes the only history, and the original moments are gone. Could ship in parallel as a complement to retrieval.

### B. Tag-based retrieval (no embeddings)

Tag each memory at write-time with topics (NPC names, locations, themes) and retrieve by tag-match. Cheap and explainable. Requires the writer (the engine, on each Tier 1/2 turn) to assign tags — that's another LLM call or a hand-coded heuristic. The heuristic loses the semantic match that's the whole point of embeddings.

### C. Cloud-only embeddings, drop local-Ollama support for long-lived NPCs

Cleanest code, violates [ADR-005](005-ollama-local-inference.md). Rejected.

## Related

- [ADR-002 Cognitive LOD Tiers](002-cognitive-lod-tiers.md) — the tier system this would slot into.
- [ADR-003 SQLite WAL Persistence](003-sqlite-wal-persistence.md) — likely host for the vector store.
- [ADR-005 Ollama Local Inference](005-ollama-local-inference.md) — local-first commitment that constrains the embedding-model choice.
- [ADR-017 Per-Category Inference Providers](017-per-category-inference-providers.md) — needs a new category for embedding.
- [docs/design/npc-system.md](../design/npc-system.md) — current memory model. The "Consider embedding-based retrieval" sentence there is the seed for this ADR.
- [LLM Quality Evals Plan](../plans/llm-quality-evals.md) — measurement framework that should land before this decision.
- `crates/parish-npc/src/memory.rs` — the module to extend.
