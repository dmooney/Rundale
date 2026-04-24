# Knowledge-Graph Grounding

**Target crate:** new `crates/parish-knowledge/` (SQLite-backed),
read/write hooks from `crates/parish-npc/`. Complements — does not replace —
semantic memory (doc 01).

## Problem

Semantic memory tells us *how* an NPC remembers. It does not tell us *what is
objectively true* and *who has been told what*. Today an NPC can reveal a
secret the game's simulation never actually told them, because the prompt
includes world knowledge for coherence. The result: rumours cannot gate
speech, because there is no authoritative record of who knows what.

## SOTA technique

A symbolic triple store with provenance and confidence:

```
(subject, predicate, object, source, confidence, learned_at)
("Séan_Ó_Conaill", "owes_debt_to", "landlord_blake", "Máire_overheard",
 0.8, 1820-06-12T09:14)
```

- **Subject/object:** NPC / place / item ids.
- **Predicate:** a small, stable vocabulary (`owes_debt_to`, `is_courting`,
  `holds_grudge_against`, `witnessed`, `owns`, `lives_at`, …).
- **Source:** the event or NPC the triple was learned from.
- **Confidence:** 0–1, decays over time; raised by corroboration.

Each NPC carries a subset view — *their beliefs*. Global ground truth lives
in a separate table and is only writable by Tier 4 / authored content.

## Why it's distinct from semantic memory

- **Semantic memory** is episodic / fuzzy, retrieved by cosine similarity,
  used for flavour.
- **Knowledge graph** is symbolic, queryable, used for *gating*: "would Séan
  actually know this?" before we let him say it.

Both can coexist: dialogue generation retrieves fuzzy context from memory
*and* queries the graph for hard facts the NPC may reference.

## Integration points

- **Post-turn extraction:** a small utility-lane model (see doc 05)
  extracts candidate triples from the NPC's own utterance
  (`(npc, learned, fact, from=player)`). Store as low-confidence until
  corroborated.
- **Pre-turn gate:** before streaming a Tier 1 response, intersect the NPC's
  graph with named entities the player just mentioned; remove anything
  from the prompt the NPC has no triple for. Drops hallucinated knowledge.
- **Gossip hop:** when doc 07's rumour-mutation pass runs, the teller
  publishes a triple to the listener; the listener accepts it at reduced
  confidence. Now gossip spread is observable — you can query "who knows
  about Máire's debt?" at any moment.
- **Quests (future):** triples are a natural substrate. A fetch quest is
  `(player, brings, item, to, npc)`; completion is a triple write.

## Minimal first cut

1. `parish-knowledge` crate with a SQLite schema of three tables:
   `fact`, `belief` (per-NPC view with confidence), `provenance`.
2. Predicate registry in a mod file (`mods/rundale/predicates.json`) to keep
   the vocabulary controlled.
3. Utility-lane extraction after every Tier 1 and Tier 2 turn; write triples
   at low confidence.
4. Prompt builder in `parish-npc` queries the per-NPC `belief` view to
   populate a "You know these things:" stanza; removes anything the NPC
   does not hold.
5. Flag `knowledge-graph`; default off until extraction stabilises.

## Effort & sequencing

Roughly 2 engineer-weeks. Slot after doc 01 (embeddings ship first so
semantic context is solid) and alongside doc 04 (tools can query the graph
directly: `believes(npc, subject, predicate)`).

## Risks

- Extraction noise produces junk triples. Gate writes on a confidence
  threshold and require corroboration (≥ 2 independent sources) before a
  belief becomes actionable in the gate.
- Predicate vocabulary drift. Keep it closed; any new predicate requires a
  mod-file change and migration.
- Query cost on hot prompt builds. Index on `(npc_id, subject)` and cache
  per-scene.

## Papers / references

- Bordes et al., *Translating Embeddings for Modeling Multi-relational Data*
  (TransE, 2013).
- Speer et al., *ConceptNet 5.5* (2017) — precedent for commonsense triple stores.
- Peng et al., *Check Your Facts and Try Again: Improving LLMs with External
  Knowledge and Automated Feedback* (2023).
- Wang et al., *Knowledge Graph Prompting for Multi-Document QA* (2024).
