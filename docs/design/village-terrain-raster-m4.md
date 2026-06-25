# Village Terrain Raster M4

## Purpose

M3 moved terrain into config and added a generated underpaint pass, but the screenshots still exposed a core limitation: many current terrain atoms are transparent PNG cutouts whose visible content nearly fills a rectangular bounding box. Repeating them across the scene creates dark or grey rectangular plates, even when topology and validation are correct.

M4 should generate natural terrain rasters from the layout config before compositing constructed objects. Ground, water, mud paths, banks, wetness, vegetation noise, and broad lighting should come from deterministic raster generation tied to the hidden grid. Cottages, bridges, walls, carts, wells, signs, smoke, and NPCs remain sprite-composited on top.

## Implementation Status

Implemented in `codex/village-scene-generator-m1` as a proof-pack generator mode, not as committed live mod content. `generate-village-layouts.mjs --asset-out ...` now writes one deterministic opaque PNG terrain raster per layout, appends generated `kind: "ground"` assets to the generated pack, and inserts a `terrain-raster` layer before constructed sprites. The pack validator rejects missing generated assets, duplicate raster signatures, broken layer references, and raster assets that are not generated ground PNGs.

The M4 contact sheet proves the repeated transparent-terrain-atom rectangles are gone and that the visual client renders without legacy plate or underlay fallback. The current terrain painter remains a procedural underpaint, though; it is a contract and topology milestone rather than final art. The final visual direction should keep this topology/asset/mask contract and replace the raw procedural painter with AI-generated isometric terrain chunks or masked underpaint tiles.

## Player Experience

The player should see outdoor village layouts that feel like coherent hand-authored pixel-art places: roads and paths connect, water flows continuously, bridges sit correctly over water, cottages sit on dry ground, props are reachable, and NPCs stand at plausible anchors. The first read should be terrain-first, not a collage of visible terrain rectangles.

## Affected Subsystems

- `mods/rundale/scene-recipes/outdoor-village-layouts.json`: keep terrain profiles and add raster-generation controls if needed: palette, wetness, path width, water width, grade, noise seed, and generated-asset naming.
- `parish/apps/visual/scripts/generate-village-layouts.mjs`: write deterministic terrain PNGs, add generated asset records to the pack, and layer raster terrain before manmade sprites.
- `parish/apps/visual/scripts/audit-scene-atoms.mjs`: reuse or extend PNG parsing/content checks for generated rasters.
- `parish/apps/visual/scripts/generate-village-layouts.test.mjs`: test deterministic raster output, signatures, coverage metrics, and negative cases.
- `.proofs/village-terrain-raster-m4/`: generated pack, summary, generated PNG assets, screenshot proof, contact sheet, evidence, and judge.
- `parish/testing/fixtures/play_village-terrain-raster-m4.txt`: live command fallback proof.

No Rust runtime schema change is required for M4 unless generated assets are promoted into committed mod content. The visual client can already render `SceneState.layers` backed by asset ids and PNG URLs.

## Raster Model

M4 should create one of these compatible outputs:

- One full-stage generated terrain raster per layout, kind `ground`, sized to the scene `native_size`.
- Or a small set of deterministic chunks, sized and anchored consistently, when chunking helps future streaming/tiling.

For M4, a full-stage terrain raster is the simpler proof. It avoids alpha seams because the terrain image is intentionally full-frame. It also mirrors how classic isometric games often separate terrain from objects: the map/terrain pass owns natural surfaces, and object sprites own readable things that can be clicked, occluded, or varied independently.

The raster generator should use the same topology data as validation:

- road/path cells and path segments for mud lanes;
- water polylines and rendered-water cells for streams/rivers/ditches;
- bridge declarations to keep water continuous beneath bridge decks;
- cottage, prop, and NPC footprints as masks/avoidance zones;
- terrain profile values for palette, wetness, vegetation density, path width, and lighting.

## Data Model

The generated pack can include generated assets in its `assets` array:

```json
{
  "id": "generated-terrain-kilteevan-layout-01-bridge-hamlet",
  "kind": "ground",
  "image": "generated-assets/01-bridge-hamlet-terrain.png",
  "anchor": [50, 50],
  "generated": true
}
```

The scene should include that asset as the first terrain layer. Existing legacy plate/underlay fields may remain for compatibility, but render proof should show the generated raster is the visual terrain source and fallback plates are not used.

Summary fields should include:

- `terrain_raster_asset`;
- `terrain_raster_signature`;
- `terrain_pixel_hash`;
- `terrain_raster_size`;
- `terrain_raster_layer_count`;
- `repeated_terrain_atom_count`;
- `raster_water_coverage_cells`;
- `raster_path_coverage_cells`;
- `rectilinear_artifact_score` or an equivalent screenshot/pixel metric.

## AI Asset Direction

M4 is not the final AI-image milestone, but it prepares the right slot for it. Once deterministic raster generation can own terrain and validation, GPT-image-generated terrain chunks can replace procedural/color-ramp placeholders without changing the compositor contract. The AI-generated assets should still enter with metadata: scale, perspective, lighting, palette, anchor, terrain tags, masks, and compatible topology.

The long-term architecture remains:

- deterministic layout/topology solver;
- generated terrain/background pass for natural surfaces;
- sprite compositor for constructed objects and NPC atom assemblies;
- validation before screenshot;
- screenshot/judge feedback loop after render.

## Feature Flag

No runtime feature flag is required while generated rasters are proof-pack outputs. If generated rasters are committed to the live mod scene index later, gate that integration behind a default-on flag such as `visual-generated-terrain-rasters`.
