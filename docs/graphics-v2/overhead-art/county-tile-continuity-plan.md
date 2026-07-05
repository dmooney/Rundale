# County-Scale Overhead Map Tile Continuity Plan

## Goal

Produce an overhead ink-and-watercolor gameplay map for a whole county while
keeping roads, paths, field boundaries, buildings, water, bog, tree belts, and
palette continuous across tile edges.

This plan assumes the current preferred direction: direct historic-map to
overhead watercolor art, with the Cycle CB `B no legend` surface and Cycle CD
`D-style pawns at 3x map scale` as the strongest gameplay direction so far.

Cycle CE adds the first practical proof:

- real Roscommon NLS z17 tiles around Murphy's Farm were assembled as a `5x5`
  source mosaic;
- independent per-runtime-tile styling produced a visible tile-grid artifact
  (`seam_to_control_ratio ~= 1.94`);
- mosaic-first styling dropped back near the raw-source baseline
  (`seam_to_control_ratio ~= 1.06`, source baseline `~= 1.10`);
- one imagegen-generated continuous supertile split cleanly into `5x4` runtime
  tiles (`seam_to_control_ratio ~= 1.01`, reassembly error `0`).
- two independently generated overlapping imagegen supertiles failed when their
  safe centers were stitched (`join_to_control_ratio ~= 2.75`);
- a follow-up imagegen seam repair reduced the seam metric back to
  `~= 1.01`, but behaved like a broad repaint and therefore needs mask/control
  discipline before production use.

This proves the first primitive: continuous supertiles can be split into
runtime tiles without adding seams. It also proves that neighboring imagegen
supertile generation cannot be trusted to match by overlap alone.

Cycle CF adds the production-shaped proof:

- `docs/graphics-v2/scripts/county_tile_pipeline.py` is the reusable CLI for
  source fetch/mosaic assembly, deterministic continuous rendering, runtime
  tile export, seam-contract generation, validation, and masked repair package
  preparation;
- the Murphy/Roscommon production proof fetched a `10x10` z17 source area
  (`100` real NLS source tiles);
- the county-base supertile exported `100` runtime tiles mechanically from one
  continuous parent artifact;
- validation passed with `max_abs_reassembly_error = 0` and
  `max_seam_to_control_ratio ~= 1.086` against the `1.15` threshold;
- the run wrote `manifest.json`, `seam-contracts.json`, `metrics.json`,
  `validation-report.json`, contact sheets, and a masked seam-repair template.
- the `repair-seam` subcommand was run on the known-failed Cycle CE adjacent
  imagegen stitch and reduced the join metric from `2.93` to `0.76` inside a
  bounded `192px` mask, while still requiring topology review.

This is the current production baseline for county coverage: deterministic
continuous county-base tiles first, imagegen reserved for high-value local art
or single large panels that can be split and validated.

## Core Rule

Do not generate runtime tiles independently.

Image models are useful for local watercolor interpretation, but they are not a
reliable source of geometry across seams. County-scale continuity needs a
deterministic world-space grid, shared source mosaic, and shared semantic layers
before image generation. Imagegen should paint inside padded windows whose
center is later cropped into runtime tiles.

Cycle CE shows why: the independent-tile workflow created obvious grid
normalization artifacts from the same source, the one-piece imagegen supertile
split cleanly, and the adjacent independent imagegen supertiles produced a
visible join until repaired.

## Continuity Types

The pipeline has to preserve four different kinds of continuity:

| Continuity | Requirement |
| --- | --- |
| Geometry | Roads, paths, rivers, drains, building footprints, and boundaries cross tile edges in the same position and width. |
| Material | A hedge, bog, road, tree belt, field, or dry-stone/bank boundary should not change material at a seam unless the source layer says it changes. |
| Style | Palette, paper texture, ink weight, watercolor granularity, and detail density remain stable across the county. |
| Runtime | Walkability, collision, sprite scale, actor anchors, and tile LODs line up exactly when the player crosses an edge. |

Geometry and runtime continuity must be deterministic. Material and style can be
imagegen-assisted but should be audited.

## Coordinate System And Tile Scheme

Use one county master coordinate space for every artifact.

- Store source rasters in a projected CRS suitable for Ireland, preferably ITM
  for authoring, with a Web Mercator export only if the renderer requires
  slippy-map compatibility.
- Define a county art grid independent of downloaded source tile boundaries.
- Assign stable IDs:
  `roscommon/{lod}/{x}/{y}@source-vN.control-vN.prompt-vN.style-vN`.
- Keep all generated art north-up and overhead. Do not let per-tile composition
  rotate or recenter the map.
- Treat game scale as a separate variable from source map scale. The current
  overhead gameplay experiments suggest a 2x-3x enlarged gameplay surface for
  readable tokens.

Recommended production grid:

- For deterministic county base, render continuous source mosaics in batches of
  at least `10x10` z17-equivalent source/runtime tiles when practical.
- For final game delivery, export `256x256` runtime tiles mechanically from
  each accepted continuous parent artifact.
- For imagegen local panels, use at least `256px` overlap on every side;
  `512px` is better where cost allows.
- Keep a seam manifest for every exported edge, even when validation passes.

## Master Layers

Every generated art panel should be derived from the same master layers:

| Layer | Source / Use |
| --- | --- |
| `source_historic_mosaic` | Highest available historic map raster, georeferenced and stitched before art generation. |
| `label_suppressed_source` | Same raster with labels/letters softened or removed so text does not become scenery. |
| `road_path_layer` | Roads, lanes, unfenced paths, yards, bridges, fords, and exits. |
| `building_layer` | Building roof footprints and significant ruins/structures. |
| `boundary_layer` | Hedges, ditches, banks, walls, fences, administrative/non-physical boundaries, and uncertainty. |
| `vegetation_layer` | Deciduous trees, conifers, mixed woods, orchards, rough pasture, scrub, crops, gardens. |
| `water_wetland_layer` | Rivers, drains, ponds, wells, marsh, bog, wet ditches. |
| `regional_material_prior` | Roscommon defaults: hedges/banks/ditches first, stone as low irregular dry fieldstone only where supported. |
| `walkability_layer` | Runtime roads/yards/paths/open fields versus blocked/slow surfaces. |

The source raster remains audit authority. The semantic layers are geometry
authority for tile seams and runtime masks.

## Seam Manifest

For each exported tile edge, store an edge contract. Neighboring tiles must
share the same contract.

```json
{
  "tile_id": "roscommon/local/123/456@source-v1.control-v1.prompt-v1.style-v1",
  "edge": "east",
  "features": [
    {
      "id": "road-connolly-lane-017",
      "class": "road",
      "edge_position_px": 318,
      "width_px": 42,
      "material": "pale dirt lane",
      "walkable": true,
      "confidence": 0.92
    },
    {
      "id": "boundary-townland-044",
      "class": "non_physical_admin_boundary",
      "edge_position_px": 710,
      "render_policy": "suppress_as_physical_feature",
      "confidence": 0.76
    }
  ]
}
```

This is more important than the generated pixels. If the art disagrees with the
edge contract, the tile is repaired or rejected.

## Generation Pipeline

1. Ingest county source.
   Build a georeferenced historic raster mosaic for the county. Preserve the
   exact source tile/sheet provenance for every pixel.

2. Build the county grid.
   Define LODs, supertile size, overlap, safe center, runtime tile size, and
   stable tile IDs. This grid does not depend on downloaded source tile seams.

3. Produce label-suppressed source.
   Remove or soften letters, numbers, and map labels, but do not erase linework
   or symbols. Store this separately from the raw source so raw map evidence is
   always available.

4. Build semantic master layers.
   Combine deterministic extraction, existing GIS where useful, the map legend,
   and manual correction. The manual map-annotator GUI is acceptable here
   because county-scale accuracy beats prompt-only interpretation.

5. Generate seam manifests.
   For every tile edge, intersect the semantic master layers with that edge and
   write the expected crossings, widths, classes, and render policies.

6. Generate padded art supertiles.
   Give imagegen the padded label-suppressed source crop, a rendered semantic
   control image, the regional material prior, and the fixed overhead style
   prompt. The model paints the larger padded panel, not the final runtime tile.

7. Crop only safe centers.
   Discard the overlap margins or use them only for blending diagnostics.
   Runtime tiles come from the center where the model had neighboring context.

8. Audit seams.
   Build 2x2 contact sheets of adjacent safe centers plus semantic edge
   overlays. Reject a tile if roads, paths, rivers, field boundaries, building
   footprints, or material classes jump at the seam.

9. Repair narrowly.
   Prefer deterministic overlay repair for roads/boundaries. Use imagegen
   repair only on a padded 2-tile or 2x2 seam patch with the seam contract shown
   as an input, then crop back to the tile grid. Cycle CE's first repair
   removed the seam but repainted broadly, so production repair should use a
   mask/semantic overlay and reject topology drift.

10. Export runtime bundle.
    Write art tiles, masks, edge manifests, provenance, style version, prompt
    version, and contact-sheet audits.

Cycle CF implements the deterministic county-base version of this pipeline in
`docs/graphics-v2/scripts/county_tile_pipeline.py`. Treat that CLI as the
current production entrypoint for county base proof runs. It also includes
`repair-seam`, a bounded local repair tool for failed adjacent-panel stitches;
that tool harmonizes seam color/texture but does not guarantee road, path,
building, or boundary alignment.

## Prompt Template

Use the same prompt for all local supertiles, with tile-specific source/control
images but no tile-specific hand-authored interpretation.

```text
Use case: overhead gameplay map tile
Asset type: Graphics V2 county overhead watercolor supertile

Input images:
- Image 1: padded historic source-map crop, label-suppressed but otherwise
  faithful. Use this as the geography and symbol evidence.
- Image 2: semantic control overlay in the same coordinate frame. It marks
  roads/paths/yards, building roof footprints, water/wetland, trees/orchard,
  rough pasture/scrub, boundaries, and non-physical administrative lines.
- Image 3: fixed overhead watercolor style sample from the approved Cycle CB/CD
  direction. Use it for palette, line weight, paper texture, and detail density,
  not for layout.

Primary request:
Create one strictly overhead, north-up, ink-and-watercolor gameplay map
supertile. Preserve the geography from Image 1 and the semantic classes from
Image 2. Paint a flat map surface, not an isometric scene.

Style:
Muted rural Irish parchment watercolor; pale dirt roads/yards; moss and straw
greens; raw umber and warm grey; fine black-brown ink; irregular hand-painted
edges; no modern UI.

Geometry:
Roads, paths, rivers, drains, building footprints, boundaries, tree belts, bogs,
and field shapes must remain in their source positions. Features crossing the
tile edge must continue cleanly off-frame. Do not recenter, rotate, simplify
into a scenic composition, or invent new crossroads.

Materials:
Roscommon boundary default is hedge, bank, ditch, earthen/stone bank, or
overgrown field edge. Use low irregular dry fieldstone only where supported.
Never render uniform rectangular block walls, ashlar blocks, bead-chain stones,
or continuous estate walls unless the control explicitly marks a major wall.

Runtime constraints:
Keep the surface neutral daylight. No people, animals, carts, smoke, weather,
labels, nameplates, speech bubbles, compass, UI, or readable text. Buildings
are flat roof-footprint map shapes, not perspective architecture. The tile must
remain suitable for actors and overlays to be drawn at runtime.
```

## Seam Repair Prompt Template

```text
Repair this overhead watercolor map seam without changing the map layout.

Inputs:
- Image 1: 2-tile or 2x2 seam patch from the current generated art.
- Image 2: semantic seam contract overlay showing exact crossing positions for
  roads, paths, boundaries, buildings, water, trees, and non-physical admin
  lines.

Request:
Blend palette, paper texture, ink weight, and watercolor texture across the
seam. Align every road/path/boundary/water/tree feature to the semantic overlay.
Do not add, remove, widen, or reroute any feature. Suppress administrative
lines as physical objects unless the overlay marks them as physical boundaries.

Avoid:
new roads, new walls, new buildings, labels, UI, people, animals, carts,
perspective, cast shadows, readable text, or style changes outside the seam
band.
```

## LOD Strategy

County scale needs more than one level of detail.

| LOD | Use | Generation |
| --- | --- | --- |
| County overview | Travel map, far zoom, orientation. | Mostly deterministic watercolor render from vector/raster layers; avoid per-field detail. |
| Parish/local | Player walking between nearby exterior nodes. | Imagegen supertiles with semantic controls and overlap. |
| Named-site closeup | Dense interaction areas such as farms, chapel, pub, crossroads. | Separate high-detail local art or enlarged overhead plates, using the same coordinate/mask system. |

The county overview should not be made from thousands of independent imagegen
tiles. It should be a low-frequency art render of the same master layers. Local
tiles can then replace it at higher zoom.

## Accuracy Rules

- Raw source and semantic layers override imagegen.
- The map legend is an interpretation reference for layer building, not a
  visible prompt input unless a controlled experiment proves it does not leak
  key symbols into art.
- Do not hand-author tile-specific prompt hints. Corrections belong in semantic
  layers or seam manifests.
- Administrative or survey boundaries are suppress-by-default as physical
  scenery.
- Walkable road/path continuity is higher priority than watercolor prettiness.
- For ambiguous rural Roscommon boundaries, default to hedge/bank/ditch rather
  than continuous stone walls.

## Validation

Minimum validation for a generated batch:

- 2x2 seam contact sheets for every generated supertile neighborhood.
- Edge-manifest overlay for each seam.
- Road/path crossing check: each crossing is within tolerance on both sides.
- Boundary material check: no sudden hedge-to-wall or wall-to-road flips.
- Label leak check: no readable source text survives as scenery.
- Runtime mask check: walkable/blocked/soft-blocked masks have no edge gaps.
- Palette check: neighboring tiles stay within agreed average color and contrast
  bands, excluding water/bog/woodland class differences.

For the first proof, generate a `3x3` local grid around Grove or Beechwood, not
the whole county. That proves seam logic, LOD behavior, and gameplay scale with
bounded cost.

## Current Production Recommendation

Use two distinct map-art layers:

1. **County base layer:** deterministic continuous rendering from historic
   source mosaics plus seam contracts. This is scalable and currently passes
   the 10x10 proof.
2. **High-value local art layer:** imagegen supertiles for named farms,
   crossroads, villages, and dense interaction areas. These can override or sit
   above the county base only after they pass seam repair/validation.

Do not use independent imagegen runtime tiles for the county base. Do not trust
adjacent imagegen panels by overlap alone.

## Next Prototype

Cycle CE completed the first proof on Murphy's Farm:

1. fetched a real `5x5` z17 NLS source mosaic;
2. compared independent-tile styling with mosaic-first styling;
3. generated one imagegen continuous supertile;
4. split that supertile into runtime tiles and verified reassembly;
5. generated two overlapping adjacent imagegen supertiles;
6. proved safe-center stitching alone fails;
7. proved a seam repair can remove the seam visually, with broad-repaint caveats.

Cycle CF completed the first production-shaped deterministic proof. The next
prototype should make the imagegen local-art path production-shaped with
human-corrected semantics:

1. Pick a `2x1` or `2x2` area around Murphy's Farm or Grove.
2. Build a master source mosaic covering both supertiles plus overlap padding.
3. Manually correct the road/path/building/boundary/tree/wetland layers in the
   map annotator for that area.
4. Generate adjacent overlapping imagegen supertiles with the same prompt,
   source/control scheme, and style reference.
5. Crop only their safe centers and build a seam contact sheet.
6. Run a masked seam-band repair with the seam contract overlay visible, or use
   the deterministic `repair-seam` tool only when the failure is color/texture
   discontinuity rather than topology drift.
7. Audit only these questions:
   - Do roads and paths cross seams exactly?
   - Do buildings and boundaries stay in source positions?
   - Does palette/detail remain stable?
   - Can D-style 3x pawns move across tile edges without scale changes?

Only after this passes should the process expand to a parish, then to the rest
of County Roscommon.
