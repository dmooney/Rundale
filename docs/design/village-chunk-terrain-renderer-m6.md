# Village Chunk Terrain Renderer M6

## Purpose

M5 made terrain auditable as connected chunks with ports and masks, but the visible proof still used M4-style full-stage raster terrain. M6 should make the chunk map visually real: generate reusable bitmap terrain chunk assets, place them as compositor layers, and prove the screenshots are assembled from those chunk sprites. This is the bridge between the current procedural proof and the desired GPT-image-generated pixel-art terrain library.

## Implementation Status

Implemented in `codex/village-scene-generator-m1` as a proof-pack generator mode. `generate-village-layouts.mjs --chunk-render-mode sprites` now writes reusable generated bitmap terrain chunks into `generated-assets/chunks/`, one generated natural ground-fill raster per layout into `generated-assets/ground/`, visible compositor layers for non-ground terrain chunks, and summary metrics proving path/water/bank/bridge/detail layers came from the chunk map.

The output is still proof-grade procedural pixel art, not final GPT-image terrain art. It does, however, make the Factorio/Stardew-style compositor mechanically real: roads, waterways, bank patches, bridge-adjacent chunks, and grass details are independent bitmap sprite layers with chunk ids, templates, masks, ports, and deterministic variant seeds.

## Player Experience

Players should see ten plausible outdoor village scenes where roads, streams, banks, bridge approaches, and worn details read like coherent isometric map pieces. The world should still have a natural base ground layer, but visible paths and waterways should be made of repeated/varied chunks that connect physically through the layout, not a single painted sheet.

## Affected Subsystems

- `parish/apps/visual/scripts/generate-village-layouts.mjs`: add chunk sprite asset generation, chunk layer creation, summary metrics, CLI flag(s), and validation.
- `parish/apps/visual/scripts/generate-village-layouts.test.mjs`: cover deterministic chunk assets, chunk layer references, missing assets, duplicate source ids, and physical topology preservation.
- `mods/rundale/scene-recipes/outdoor-village-layouts.json`: may gain chunk render defaults such as chunk sprite size, anchor, variant count, and layer ordering.
- `.proofs/village-chunk-terrain-renderer-m6/`: generated pack, summary, chunk map, generated chunk PNGs, screenshots, transcript, evidence, and judge.
- `parish/testing/fixtures/play_village-chunk-terrain-renderer-m6.txt`: live fallback proof.

## Data Model

The pack should keep the M5 chunk map as the authoritative topology contract and add visible compositor layers that reference the chunks:

```json
{
  "id": "terrain-chunk-bridge-hamlet-path-12-8",
  "kind": "ground",
  "asset_id": "terrain-chunk-path-straight-ns-v03",
  "terrain_chunk_id": "bridge-hamlet-path-12-8",
  "terrain_chunk_class": "path",
  "terrain_chunk_template": "path-straight-ns",
  "anchor": "center",
  "position": [640, 360],
  "z": 120
}
```

Generated summaries should expose:

- `terrain_chunk_render_mode`
- `terrain_chunk_sprite_layer_count`
- `terrain_chunk_sprite_asset_count`
- `terrain_chunk_sprite_class_counts`
- `terrain_chunk_sprite_missing_assets`
- `terrain_chunk_sprite_path_coverage_cells`
- `terrain_chunk_sprite_water_coverage_cells`
- `terrain_chunk_sprite_collision_count`
- `terrain_chunk_sprite_signature`

## Rendering Approach

M6 should prefer real bitmap chunks even if the art is still generated procedurally. The safe path is to produce transparent PNG sprites for each chunk template/variant into the proof asset directory, then create one compositor layer per chunk. A muted natural base raster can remain underneath as ground fill, but the visible path/water/bank/detail surfaces must come from chunk layers.

Legacy visual-water exclusion masks remain in the hidden topology model for collision safety, but M6 does not render bank chunks whose source is only that legacy mask. Visible bank sprites are tied to actual configured waterways; otherwise dry layouts show a misleading grid.

## AI Asset Direction

The chunk assets produced in M6 are still proof assets. Their metadata should be shaped so a later GPT-image pipeline can replace them template-by-template: template id, terrain class, ports, compatible neighbors, pixel density, anchor, mask, palette, lighting, and variant seed. The important product decision is that future villages scale by picking validated atoms from a catalog, not by asking arbitrary sprites to land plausibly after the fact.

## Feature Flag

No runtime feature flag is required while M6 remains a proof-pack generator mode. If chunk sprite terrain becomes committed live content, gate it behind a default-on flag such as `visual-terrain-chunk-sprites`.
