# RAG Lore Recall for NPC Dialogue

> Parent: [Architecture Overview](../overview.md) | [Docs Index](../../index.md) | Related: [NPC System](../npc-system.md), [Inference Pipeline](../inference-pipeline.md), [NPC Prompt Immersion Ideas](npc-prompt-immersion-ideas.md)

> Status: **Proposed** (implementation parked behind PR #443). Tracking branch `claude/rag-recall-integration-5zVsN`; companion implementation plan in `docs/plans/` / the agent plan file.

## Problem

When the player speaks to an NPC, the Tier 1 prompt is assembled from a fixed set of inputs (see [NPC System](../npc-system.md)):

1. The NPC's identity, personality, and mood (`build_tier1_system_prompt`).
2. Relationship summaries ("PEOPLE YOU KNOW").
3. The four hand-authored `knowledge` bullets per NPC in `mods/rundale/npcs.json` ("WHAT'S ON YOUR MIND").
4. Per-turn context: location, time, recent conversation, short-term memory, a **keyword-based** long-term-memory recall (`LongTermMemory::recall_context_string`, `crates/parish-npc/src/ticks.rs`), and gossip.

The world's actual lore is far richer than this: `mods/rundale/{world,npcs,festivals}.json` contain location folklore, mythological significance, festival dates and meanings, and the biographies and relationships of every other NPC. None of it reaches the prompt unless it happens to be one of the four `knowledge` bullets. So when a player asks Padraig "what is Lughnasa?" the model either answers from its own training data (often wrong for a 1820 Irish parish) or hallucinates.

Two existing recall mechanisms fall short:

- **Hand-authored `knowledge` bullets** — high quality but only four per NPC; they can't cover the breadth of the parish.
- **Keyword long-term-memory recall** — only retrieves *that NPC's personal memories*, and only on literal token overlap (words > 4 chars). It can't surface shared world lore, and synonyms miss.

## Goal

Ground Tier 1 dialogue in the parish's own JSON lore. Each turn, retrieve the top-k semantically relevant lore passages for the player's question and inject them into the NPC's context as a distinct **"KNOWLEDGE YOU RECALL"** block, so NPCs answer from parish canon instead of guessing.

## Approach

A small retrieval-augmented-generation (RAG) layer, factored as a standalone `parish-rag` crate, prototyped in PR #486. The crate is deliberately minimal — a demo of the pattern, not a vector database.

```
mods/rundale/*.json ──► build_rundale_corpus ──► Vec<LoreChunk>   (one fact per chunk, ~280 chunks)
                                                      │
                                          AnyEmbedder::index (embed each chunk)
                                                      ▼
                                                  LoreIndex            (in-memory, cosine top-k)
                                                      │
 player input ──► AnyEmbedder::embed ──► query vec ──┤
                                                      ▼
                                          LoreIndex::search(query, k)
                                                      │
                                          format_recall_block(hits)
                                                      ▼
                              "KNOWLEDGE YOU RECALL:\n- <fact>\n- <fact>…"  appended to Tier 1 context
```

### Components

| Component | Responsibility |
| --- | --- |
| `LoreChunk` / `build_rundale_corpus(mod_dir)` | Read the mod's JSON and split it into one-fact-per-chunk passages: per location (description + folklore as separate chunks), per NPC (identity + personality + each knowledge entry + each relationship), per festival. Chunk granularity is the lever that keeps a single recall from blowing out the prompt. |
| `AnyEmbedder` | Unified handle over embedding backends, mirroring `parish-inference::AnyClient`. Two variants: `HashEmbedder` and `OllamaEmbedder`. |
| `LoreDocument` / `LoreIndex` | An embedded chunk and the in-memory vector store. `search()` is a linear cosine-similarity scan returning the top-k — fine for a few hundred chunks. |
| `format_recall_block(hits)` | Render retrieved hits as the "KNOWLEDGE YOU RECALL (things you know from living here):" block. Returns an empty string when there are no hits, so callers append unconditionally. |

### Embedders

- **`HashEmbedder` (default)** — deterministic hashing-trick embedder. Tokenises, drops stopwords, hashes each token into a fixed-width L2-normalised vector with a signed hash. No network, no model, byte-identical across runs — which makes it the embedder used in tests and the offline default. Limitation: it captures *token overlap*, not semantics, so synonym queries won't match.
- **`OllamaEmbedder`** — calls Ollama's `/api/embeddings` (e.g. `nomic-embed-text`) for genuine semantic retrieval. This is the production-quality path when a local Ollama is available.

The split mirrors the rest of the project: deterministic offline fallback for reproducibility, real model for quality. RAG keeps its *own* embedding handle rather than reusing `AnyClient`, because embeddings hit a different endpoint with a different request shape than chat completions; coupling them would force an awkward abstraction.

## Where it plugs in

Retrieval happens in **per-turn context assembly**, not the system prompt:

- Injected inside `build_enhanced_context_with_config` (`crates/parish-npc/src/ticks.rs`), immediately **after** the existing keyword long-term-memory recall and **before** gossip context.
- Rationale: the query changes every turn, so there is no caching benefit to putting it in the (otherwise stable) system prompt; and placing it next to the existing LTM recall reads naturally. The "KNOWLEDGE YOU RECALL" header is intentionally distinct from the LTM "You recall: …" header so the model can tell *parish lore* apart from *this NPC's personal memories*.

### The async boundary

`AnyEmbedder::embed` is async; `build_enhanced_context_with_config` is sync. Rather than make the context builder (and every caller) async, the **caller pre-embeds** the player query and passes an `Option<&[f32]>` plus an `Option<&LoreIndex>` into the otherwise-sync builder. This mirrors how LTM recall already works — the caller does the I/O, the builder only formats strings — and keeps the blast radius of the signature change small.

## Configuration & feature flag

Two independent knobs:

- **`rag-recall` feature flag** — toggles whether the feature exists at runtime. Per repo convention (CLAUDE.md rule 6) the feature ships **default-on**. Because `FeatureFlags::is_enabled` returns false for unset flags, the flag is *seeded* to `true` at first config load (a new `FeatureFlags::seed_default` helper), after which every check site is a plain `config.flags.is_enabled("rag-recall")`. Disable with `/flag disable rag-recall`.
- **`[engine.rag]` config section** — tunes *behaviour*, not existence:

```toml
[engine.rag]
embedder    = "hash"              # "hash" | "ollama"
embed_model = "nomic-embed-text"  # used when embedder = "ollama"
# ollama_url = "http://localhost:11434"   # optional override
top_k       = 4                   # passages retrieved per turn
min_score   = 0.0                 # drop hits below this cosine score
```

## Cross-frontend wiring (mode parity)

CLAUDE.md rule 2 requires CLI, web server, and Tauri to share behaviour, and shared logic to live in `parish-core`. So the corpus-load + embed + index build is a single shared helper (`parish-core::rag_init::{build_embedder, build_lore_index}`), and each frontend only does thin wiring:

- The index is built **once at startup** (gated on the flag) and handed to the `NpcManager` as an `Option<Arc<LoreIndex>>`. An `Arc<AnyEmbedder>` is stored alongside so the per-turn dialogue path can embed the player query.
- **CLI** (`headless.rs`): build after `NpcManager::load_from_file`, store on `App`.
- **Server** (`lib.rs`/`session.rs`/`state.rs`): build once on `GlobalState`; each session's `NpcManager` borrows the shared `Arc`.
- **Tauri** (`lib.rs`): build during setup; store on `AppState`. With the Hash embedder the build is effectively instant; the Ollama path is best-effort/deferred.

## Design decisions & trade-offs

1. **Retrieve into context, not system prompt** — fresh per-turn query beats prompt-cache reuse here.
2. **No per-NPC filtering (MVP)** — every NPC may retrieve any chunk, including other NPCs' bios. Filtering out `npc:<other>` chunks risks hiding genuinely-public facts ("Padraig is the publican") and needs a relationship-graph dependency the crate doesn't have. We trust the LLM to ignore irrelevant recalled facts and revisit if a prove-script shows concrete leakage.
3. **RAG owns its embedding handle** — avoids bending the chat-completion `AnyClient` around a different endpoint.
4. **Default to the Hash embedder** — works in any sandbox with zero setup; semantic quality is opt-in via Ollama.

## Known limitations

- **No index rebuild on mod reload.** The corpus is embedded once at startup; reloading mods keeps the stale index.
- **Hash embedder is lexical, not semantic.** Synonym queries miss until the Ollama embedder is configured.
- **No per-NPC scoping** (see decision 2).
- **Linear search.** Fine for ~hundreds of chunks; would need an ANN index at mod-scale of thousands.

## Relationship to the emotion system (PR #443)

This work is sequenced **after** PR #443 (the structured emotion system). The two don't conflict semantically — emotion shapes *how* an NPC speaks, lore recall shapes *what facts* they have — but they touch the same plumbing (`ticks.rs`, `manager.rs`, `engine.rs`, the harness, and the server/Tauri dialogue wiring). #443 is larger and further along, so RAG lore recall lands on top of it. In the assembled Tier 1 prompt the two compose cleanly: the emotion preamble sets tone, and the "KNOWLEDGE YOU RECALL" block supplies grounded facts.

## Verification

- **Unit:** flag-off / no-index path omits the block; flag-on with the Hash embedder injects a relevant chunk (`crates/parish-npc/src/ticks.rs` tests, plus the `parish-rag` crate's own tests).
- **Harness:** assert the seeded flag is on, capture the assembled prompt for a canned NPC turn, confirm "KNOWLEDGE YOU RECALL" is present; disable the flag and confirm it disappears.
- **Gameplay proof:** a `/prove rag-recall` script asks Padraig "What is Lughnasa and when is it celebrated?" and asserts both that the recall block appears in the prompt and that the festival's date is referenced in the response.

## Related

- [NPC System](../npc-system.md) — entity model and context construction this hooks into
- [Inference Pipeline](../inference-pipeline.md) — prompt assembly and model selection per tier
- [Cognitive LOD System](../cognitive-lod.md) — Tier 1 is the only tier this targets initially
- [Emotion-Driven Dialogue and Simulation](emotion-driven-dialogue-and-simulation.md) — the complementary "how they speak" layer
- [NPC Prompt Immersion Ideas](npc-prompt-immersion-ideas.md) — broader prompt-grounding ideas
