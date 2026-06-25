# Village Terrain Chunk Grammar M5

## Purpose

M4 proved that topology-derived terrain can be generated as deterministic raster assets and rendered without legacy plate fallback. It did not yet produce final-feeling terrain art: the object sprites are strong, while the ground/water/path layer still reads as procedural underpaint. M5 introduces an explicit terrain chunk grammar so the layout solver can choose connected isometric pieces with ports and masks. The current painter can remain proof-grade, but the data contract should be ready for GPT-image-generated terrain chunks.

## Implementation Status

Implemented in `codex/village-scene-generator-m1` as generator/proof-pack infrastructure. `generate-village-layouts.mjs --chunk-map-out ...` now writes a chunk-map bundle beside the generated scene pack. Each layout records hundreds of deterministic terrain chunks across `ground`, `path`, `water`, `bank`, `bridge`, and `detail` classes, with template ids, ports, masks, source ids, variant seeds, bridge under-span records, and collision summaries.

This is an architecture milestone, not final terrain art. The proof screenshots still render from M4-style generated rasters, but the terrain can now be audited and replaced chunk-by-chunk by AI-generated isometric assets. The stricter chunk-mask validation already found and fixed one real layout bug: the `forked-green` NPC slot named `west-bank` occupied a water grid cell and was moved to the dry `west-door` node.

## Player Experience

Players should see varied outdoor village scenes where paths and water behave like authored map surfaces: paths connect to doors and exits, streams continue under bridges, banks line water, cottages and carts stay on dry reachable ground, and NPCs stand on plausible walkable cells. The scene should still be a graphical adventure view, not a dashboard or a loose collage.

## Affected Subsystems

- `mods/rundale/scene-recipes/outdoor-village-layouts.json`: may gain a `terrain_chunk_grammar` section with template ids, style tags, port/mask declarations, and class weights.
- `parish/apps/visual/scripts/generate-village-layouts.mjs`: add chunk-map generation from the existing grid terrain model; optionally render chunk-mode terrain assets from the chunk map for proof screenshots.
- `parish/apps/visual/scripts/generate-village-layouts.test.mjs`: validate chunk determinism, port continuity, bridge under-spans, missing templates, duplicate chunks, and object/NPC footprint masks.
- `.proofs/village-terrain-chunk-grammar-m5/`: generated pack, summary, chunk-map JSON, generated PNG assets, screenshots, contact sheet, evidence, and judge.
- `parish/testing/fixtures/play_village-terrain-chunk-grammar-m5.txt`: live command fallback proof.

## Data Model

A generated chunk map should be separate from the compositor layer list but traceable to it:

```json
{
  "layout_id": "bridge-hamlet",
  "chunks": [
    {
      "id": "bridge-hamlet-water-10-14",
      "cell": [10, 14],
      "class": "water",
      "template": "stream-ew",
      "ports": ["west", "east"],
      "mask": { "water": true, "walkable": false, "blocks_objects": true },
      "variant_seed": "..."
    }
  ]
}
```

Important invariants:

- water chunks form one connected component per waterway;
- path chunks form a connected walkable component from entry to exits/doors;
- bridge chunks sit over water chunks and connect path ports across the water;
- banks surround water edges but do not replace water under bridges;
- cottage/cart/NPC footprints must not overlap water or blocked chunks;
- every generated chunk has a deterministic template and variant seed.

## AI Asset Direction

M5 should define the metadata GPT-image-generated assets need: template id, class, ports, mask, anchor, perspective, pixel density, palette family, lighting, and compatible neighbor tags. Once this exists, the art pipeline can generate many variants per template: multiple cottage angles, water bends, road forks, bank edges, bridge approaches, wall runs, and NPC clothing atoms. The game then scales by selecting validated atoms from metadata rather than asking arbitrary sprites to make physical sense after placement.

## Feature Flag

No runtime feature flag is required while M5 remains a proof-pack generator mode. If chunk-mode terrain is promoted into committed live mod content, gate it behind a default-on flag such as `visual-terrain-chunk-grammar`.
