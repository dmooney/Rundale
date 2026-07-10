# Notebook Person Art Pipeline

This folder contains the reviewed source inputs for the illustrated-notebook
person art slice (#1628).

## Metadata Export

The upstream art-input dataset is generated from canonical NPC/world data plus
the reviewed art-direction supplement:

```sh
cargo run --manifest-path parish/Cargo.toml -p parish-npc-tool -- art-inputs \
  --npcs mods/rundale/npcs.json \
  --world mods/rundale/world.json \
  --art-direction parish/apps/ui/art/notebook-person-art/npc-art-direction-v1.json \
  --output parish/apps/ui/art/notebook-person-art/npc-art-inputs-v1.json
```

The export covers all 23 current Rundale NPCs. The audit is
`npc-art-data-audit-v1.md`; the authoring rules are
`docs/graphics-v2/npc-portraits/art-metadata-guidelines.md`.

## Rerun

```sh
pnpm --dir parish/apps/ui run notebook:people
```

This downstream assembly command reads `approved-cast-v1.json`, validates that
every source sheet and person entry is explicitly `approved`, crops the reviewed
source sheets, chroma-keys the marker sprites, writes stable runtime PNGs under
`static/rundale/notebook-ui/people/`, updates `asset-manifest.json`, and writes
`static/rundale/notebook-ui/person-art-contact-sheet.png`.

It is not the provider-generation stage. The provider-generation stage must
consume `npc-art-inputs-v1.json`, call the configured image provider/model,
store candidates and receipts, and promote only reviewed/approved assets.

## Review Gate

Generated candidates are not treated as approved by default. A source sheet,
fallback, or person entry with any `approval_status` other than `approved`
causes the pipeline to fail. The config stores the source prompt, source sheet,
runtime asset paths, cell coordinates, and per-entry review notes.

The only visual authority for this issue is
`docs/graphics-v2/illustrated-parish-notebook.png`. Existing portrait
experiments, marker concept sheets, old procedural busts, and placeholder
markers are not source artwork for this approved set.

## Approved Initial Set

The first approved set covers the live starting Kilteevan cast plus the
early/common notebook people used by the selected-person UI:

- Brigid Ni Fhatharta
- Sean Ruadh Kelly
- Peig Hannigan
- Roisin Connolly
- Aoife Brennan
- Mick Flanagan
- Niamh Darcy
- Unknown parish neighbour fallback
