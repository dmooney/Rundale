# parish-rag

Retrieval-augmented generation for Parish NPC knowledge.

Given a player question, this crate retrieves the most relevant passages from a
lore corpus (world locations, NPC biographies, festivals) and injects them into
an NPC's system prompt as **recalled knowledge** — so the NPC can answer
grounded in their world instead of hallucinating.

The crate is a standalone demo of the pattern. It does not replace the keyword
recall in `parish-npc::memory`; it complements it.

## Quick demo (offline, no Ollama required)

```sh
cargo run -p parish-rag --bin npc_knowledge_demo
```

This runs through a scripted set of questions against Padraig Darcy (the
publican) using a deterministic hashing-trick embedder — no network, no
external model.

For each question the demo prints:

1. The query.
2. The top-k retrieved lore passages with cosine similarity scores.
3. The baseline and RAG-enhanced system prompts (char counts).

## Live demo (Ollama + LLM)

```sh
cargo run -p parish-rag --bin npc_knowledge_demo -- \
    --embedder ollama \
    --embed-model nomic-embed-text \
    --chat-model qwen2.5:7b \
    --llm
```

With `--llm` the demo calls the chat endpoint twice per question — once with
the baseline prompt, once with the RAG prompt — and prints the two responses
side by side. The difference is the whole point.

## Pointing the demo at another NPC or question

```sh
cargo run -p parish-rag --bin npc_knowledge_demo -- \
    --npc "Siobhan Murphy" \
    --question "Who should I see about renting farmland?"
```

## What gets indexed

Chunks are built from `mods/rundale/`:

| Source             | Chunk template                                                             |
| ------------------ | -------------------------------------------------------------------------- |
| `world.json`       | One chunk per location description; one per folklore/mythological note.    |
| `npcs.json`        | Identity, personality, each `knowledge` entry, and each relationship.      |
| `festivals.json`   | One chunk per festival.                                                    |

Fine-grained chunks keep each retrieval focused: retrieving "Padraig is the
publican" should not also drag in his entire schedule.

## Public API

- `build_rundale_corpus(mod_dir)` — load and chunk the Rundale mod.
- `AnyEmbedder` — `Hash` (offline) or `Ollama` (live).
- `LoreIndex::search(query_vec, k)` — cosine-similarity top-k retrieval.
- `format_recall_block(hits)` — builds the "KNOWLEDGE YOU RECALL" block to
  append to an NPC system prompt.

## Tests

```sh
cargo test -p parish-rag
```

All tests run offline and assert on deterministic output.
