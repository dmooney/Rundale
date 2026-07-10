# NPC Art Data Audit V1

Issue: #1628, deliverable 1 metadata drilldown.

## Decision

The existing canonical NPC/world data is not sufficient by itself for
production notebook person art. It is good identity data, but not a complete
visual contract.

Current result:

- 23 NPCs in `mods/rundale/npcs.json`.
- 0/23 are fully production-ready from canonical NPC/world data alone.
- 11/23 are strong partials with several useful visual cues.
- 12/23 are weak partials that mainly identify age/role/context.
- 23/23 now have complete reviewed art-direction records in
  `parish/apps/ui/art/notebook-person-art/npc-art-direction-v1.json`.
- 23/23 now export to generator-ready records in
  `parish/apps/ui/art/notebook-person-art/npc-art-inputs-v1.json`.

## Command

```sh
cargo run --manifest-path parish/Cargo.toml -p parish-npc-tool -- art-inputs \
  --npcs mods/rundale/npcs.json \
  --world mods/rundale/world.json \
  --art-direction parish/apps/ui/art/notebook-person-art/npc-art-direction-v1.json \
  --output parish/apps/ui/art/notebook-person-art/npc-art-inputs-v1.json
```

## Sufficiency Findings

Canonical data includes:

- age, pronouns, occupation, mood
- brief description
- home/workplace
- schedule and activity context
- relationships and knowledge
- global setting from `mods/rundale/mod.toml` and world/prompt files

Canonical data does not consistently include:

- stable face/hair identity
- stable clothing and class/work cues
- portrait pose/expression
- marker silhouette
- large readable marker props
- palette notes
- per-NPC avoid-list
- fallback identity rules

Therefore the production path needs the separate art-direction supplement. The
supplement is source data, not approved generated art.

## Strong Partials

These have relatively strong canonical visual cues but still require the
supplement for production consistency:

- Seamus Gallagher
- Maire Gallagher
- Colm Gallagher
- Nora Duffy
- Brendan Duffy
- Eamon Walsh
- Ciaran Walsh
- Brigid Ni Fhatharta
- Sean Ruadh Kelly
- Peig Hannigan
- Martin Concannon

## Weak Partials

These identify the NPC but do not provide enough structured visual information
for production art without supplementation:

- Padraig Darcy
- Siobhan Murphy
- Fr. Declan Tierney
- Roisin Connolly
- Tommy O'Brien
- Aoife Brennan
- Mick Flanagan
- Niamh Darcy
- Cormac Duffy
- Kathleen Walsh
- Liam Murphy
- Una Malone

## Boundary

This audit completes the metadata/input portion of deliverable 1. It does not
claim the generated artwork exists yet. The next stage must consume
`npc-art-inputs-v1.json`, call the configured image provider/model, store
candidate outputs and receipts, run review/approval, then promote approved
portrait/marker assets into the runtime pack.
