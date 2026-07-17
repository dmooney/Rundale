# NPC Art Data Audit

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
- 23/23 now have schema-v4 art-direction records in
  `parish/apps/ui/art/notebook-person-art/npc-art-direction-v1.json`.
- 23/23 now export to generator-ready records in
  `parish/apps/ui/art/notebook-person-art/npc-art-inputs-v1.json`.
- Every named record and fallback has a unique stable identity seed, cohort,
  nine explicit facial-geometry dimensions, at least two distinguishing
  features, provider-facing hair prose, structured hair/headwear topology,
  clothing, expression, optional portrait props, and character-only marker cues.
- The exporter rejects blank identity fields, duplicate seeds/fingerprints, and
  same-cohort faces that differ in fewer than four of nine geometry dimensions.
  It independently rejects hair/headwear topologies that differ in fewer than
  two of front, rear, covering, and silhouette family.

## Corrected Finding

The first audit incorrectly treated one populated `face_and_hair` sentence per
NPC as a complete appearance contract. Full-cast review disproved that claim:
the younger and middle-aged women mostly had mood adjectives, covered/pinned
hair, shawls, and aprons, but no face shape, proportions, eye spacing, nose,
mouth, jaw, cheekbones, hairline, or age-line geometry. Eight generated women
collapsed onto one Roisin-like face.

That failure was a data-contract bug, not merely image-model variance. Schema v2
replaces `face_and_hair` with structured facial identity and cast-level collision
validation. A controlled provider ablation also proved that the shared full-face
Roisin references overpowered the enriched records, so production generation now
uploads only the authoritative full notebook concept.

A second full-cast review found a separate failure: although the v2 faces were
distinct, seven of ten women explicitly had a centre or near-centre part and
nearly every record ended in a low knot, low coil, or covered low arrangement.
The model correctly amplified that repetition into one dominant hairstyle.

Five-whys root cause:

1. The generated women shared a centre-parted low-bun silhouette because their
   provider-facing hair sentences repeatedly requested it.
2. The sidecar encoded hair as one free-text field, so small wording changes
   such as "knot" versus "coil" looked different to the validator.
3. The collision gate counted the entire sentence as one identity dimension;
   it could not compare visible hair topology.
4. The first correction concentrated on facial geometry after the face-collapse
   failure and did not model hairstyle construction independently.
5. The repository rule required hair to be separate from facial geometry but
   did not require machine-comparable front, rear, covering, and silhouette
   categories.

Schema v3 fixes the root: facial and hairstyle collisions are now independent,
every record supplies four machine-comparable topology families plus literal
descriptions, and the provider-facing prose must compose them faithfully. The
structured topology is intentionally internal and omitted from the stable v2
provider-input export, so lint-only metadata changes do not invalidate millions
of unaffected paid jobs.

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

- stable facial geometry, distinguishing features, and hair identity
- stable clothing and class/work cues
- portrait pose/expression
- marker silhouette
- character-only marker silhouette, stance, empty-hand pose, and intrinsic
  readability cues
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

This audit establishes the corrected metadata/input contract for deliverable 1.
It does not approve generated artwork. The next stage must consume
`npc-art-inputs-v1.json`, call the configured image provider/model, store
candidate outputs and receipts, reject cast-level lookalikes, run human
review/approval, then promote approved portrait/marker assets into the runtime
pack.
