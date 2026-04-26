# LLM-Assisted Mod Authoring

**Target crate:** new `crates/parish-authoring/` (binary), invoked by the
Designer Editor (`apps/ui/`) and the `parish-geo-tool` CLI.

## Problem

A new scenario (Ulster 1798, Galway 1847, a fictional Donegal townland)
today requires hand-editing `world.json`, `npcs.json`, schedules,
relationships, and anachronism allowlists. It is weeks of work per parish
and blocks community authoring. The existing `parish-geo-tool` handles geography;
characters and relationships are still bespoke.

## SOTA technique

A toolchain that treats mod content as *LLM-fillable holes* inside
author-provided anchors:

- Author pins: parish bounds, a handful of anchor NPCs, a few anchor
  relationships, year, historical constraints.
- LLM expands: residents with coherent occupations, believable relationship
  graph, period-appropriate names, schedules that respect the world graph
  from `world.json`.

This is strictly a tool; no output is ever committed without a human review
step. The goal is 10× authoring throughput, not autonomous content.

## Concrete pipeline

1. **Geography** — `parish-geo-tool` + OSM (ADR-011) produces locations.
2. **Population prompt** — a tool-use model is given the location graph, the
   year, and the anchor NPCs. It emits candidate residents with:
   - Period-appropriate forename + surname (sampled from a curated surname
     distribution per county).
   - Occupation drawn from an authored set per location type
     (farmer/tenant/smallholder at a clachán; shopkeeper/artisan at a town).
   - Age, marital status, kinship edges to anchors when plausible.
3. **Schedules** — a second pass generates weekday/market-day/Sunday
   schedules constrained by occupation + the location graph.
4. **Relationships** — a final pass proposes a sparse relationship graph
   (avoid Dunbar blowouts) using kinship + proximity + occupation overlap.
5. **Diff view** — output is rendered as a diff in the Designer Editor;
   author accepts, rejects, or edits per-entry before commit.

## Guardrails

- **Anachronism:** anachronism word-lists (doc 03) run on every generated
  string; fails are regenerated, not silently dropped.
- **Historical plausibility:** a small judge pass (doc 09) rejects
  population densities, class mixes, or literacy levels that violate a
  `mods/<scenario>/historical-constraints.toml` table.
- **Determinism:** (scenario-id, seed) produces identical output for a given
  model revision, so authors can iterate without churn.
- **Provenance:** every generated field carries `generated_by: model_id@rev`
  metadata in the mod file so future migrations can distinguish authored
  from generated content.

## Reuse of existing infra

- `parish-geo-tool`'s `real | manual | fictional` split + `relative_to` subordination
  maps cleanly to "anchor, then fill". The geo side is already pinned by the
  user; the authoring tool fills the population around it.
- `pronunciations.json` + the generated names pipeline share input: names
  generated here auto-populate pronunciation seeds.
- `mods/rundale/anachronisms.json` is reused verbatim as the validation gate.

## Adversarial review in CI

Use the adversarial-fuzzing harness from doc 09: load a generated mod into
a throwaway game and have an adversary agent try to break continuity,
elicit anachronisms, or produce unreachable locations. Failed scenarios
fail the authoring pipeline before a PR is opened.

## Minimal first cut

1. `parish-authoring` binary with subcommands `populate`, `schedule`,
   `relate`. Claude tool-use mode (cloud) as the first backend.
2. A single worked example: `parish-authoring populate --scenario
   kiltoom-1820` producing a review-ready diff.
3. Designer Editor integration (button: "Generate candidates") that opens
   the diff viewer.

## Effort & sequencing

Roughly 2 engineer-weeks for v1. Best after doc 02 (grammar) so outputs are
schema-clean from the start, and after doc 09 (adversarial fuzzing) so
generated content has a quality gate.

## Risks

- Homogenised output — generated villages start to feel samey. Mitigate by
  sampling from distributions, using per-parish authored seeds, and
  rotating model temperature.
- Historical inaccuracy compounding. Treat every generated fact as a
  suggestion; the review step is non-negotiable.
- License / provenance of name and surname corpora — prefer public-domain
  sources (19th-century census abstracts) and document per scenario.

## Papers / references

- Smith & Whitehead, *Analyzing the Expressive Range of a Level Generator*
  (2010) — review for procedural authoring of humane content.
- Kreminski & Wardrip-Fruin, *Gardening Games: A Computational-Creativity
  Perspective on PCG* (2018).
- Todd et al., *GPT-4 Plays Minecraft: Open-Ended Game Design with LLMs*
  (2023).
- Park et al., *Generative Agents* (2023) — population authoring via LLM.
