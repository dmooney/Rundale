# Map To Background Plate Research Plan

This is a brainstorm and experiment log for a no-per-location-hints pipeline.
The goal is:

```text
historic map crop + generic legend + repeatable preprocessing
  -> consistent north-up 3/4 orthographic background plate
```

The image model should paint and resolve plausible historical material detail.
It should not be the only system deciding topology.

## Non-Negotiables

- No hand-authored per-location road, wall, river, building, or landmark hints.
- North-up ground plan: source-map top remains final-image top.
- Fixed 3/4 orthographic/isomorphic camera and sprite scale across locations.
- Static base plate only: no visible smoke, fog, weather, UI, labels, or text.
- Output must align to machine-readable movement/collision/occlusion layers.

## Pipeline Shape

1. Source crop preparation.
   - Start from a north-up historic OS map crop.
   - Normalize crop size from desired plate scale, not arbitrary map zoom.
   - Deskew, deblur lightly, remove modern map-app overlays if present.
   - Save source pixel-to-world metadata where possible.

2. Map cleanup and ignore masks.
   - Separate paper/stipple/noise from printed ink.
   - Mask text labels, numbers, survey marks, benchmark symbols, and app pins.
   - Keep the raw map as evidence, but stop text/symbols from becoming houses.

3. Feature extraction.
   - Buildings: connected components, rectangularity, hatch/fill detection,
     orientation, footprint grouping.
   - Roads/lanes: paired-line corridors, consistent width, continuity, edge
     exits, junction topology.
   - Boundaries: single/dotted/pecked/broken lines, enclosure closure, field
     edges, wall/hedge/ditch candidates.
   - Water: map-symbol detection plus an external hydrography source when
     available; do not hallucinate water from roads.
   - Trees/planting: template matching for broadleaf/conifer marks, orchard row
     patterns, planted enclosures.

4. Semantic vector layout.
   - Emit GeoJSON-like vectors with class, confidence, and source pixels.
   - Keep uncertain features as uncertain; do not silently promote them to
     roads.
   - Build a route/boundary topology graph from detected features, but do not
     pass human-written graph notes to the image model.

5. Deterministic control render.
   - Render a fixed north-up 3/4 orthographic scaffold from the vectors.
   - Use one pixels-per-meter value and one camera transform across all tiles.
   - Draw roads as walkable corridors, boundaries as raised lines, buildings as
     extruded volumes, gates/openings where routes cross boundaries, and trees
     beside routes.
   - Export control images: color semantic mask, line-art scaffold, height/depth
     map, occlusion mask, and optional navmesh overlay.

6. Image generation.
   - Inputs: raw map crop, generic legend, deterministic control render/masks,
     approved style reference, generic prompt.
   - The model paints period material, texture, vegetation, readable facades,
     and terrain charm while respecting the scaffold.
   - It receives no per-location written interpretation.

7. Automated verification.
   - Compare generated plate to vectors/control masks.
   - Fail if roads disappear, extra walkable-looking paths appear, buildings
     move too far, gates vanish, text/labels appear, smoke appears, water is
     invented, or camera metrics drift.
   - Failed tiles either rerun with the same inputs or go to map labelling /
     extraction improvement, not prompt hinting.

8. Game asset package.
   - Save background plate.
   - Save navmesh, hotspot polygons, building/door anchors, depth/occlusion
     layer, dynamic-effect sockets, and source-map provenance.

## Ideas To Test

- A. Raw map + generic prompt + style reference.
  - Lowest engineering cost.
  - Likely fails camera consistency and linework interpretation.

- B. Raw map plus a fixed oblique warp.
  - Tests whether a simple geometric transform anchors north-up camera/scale.
  - Risk: the model copies map text/noise and still lacks building volumes.

- C. Raw map plus semantic mask.
  - Tests whether rough class colors help the model separate buildings, trees,
    and linework.
  - Risk: weak extraction creates false buildings or false paths.

- D. Raw map plus extruded blockout.
  - Tests whether building/facade proportions and ground-plane scale become
    stable when the model sees a precompiled 2.5D scaffold.
  - Risk: bad detections become persuasive bad geometry.

- E. Procedural plate first, model paint second.
  - Render the entire scene as a simple game board with correct topology, then
    use image generation as style transfer/paintover.
  - This is likely the most production-friendly path if control fidelity is
    supported by the image model.

- F. Segmentation model first.
  - Train or fine-tune a small model on labelled historical OS crops to output
    roads, walls, buildings, water, trees, text, and ignore masks.
  - Higher setup cost, probably necessary for 100+ locations.

- G. External data fusion.
  - Use OSI/OS historical map plus modern/historical GIS where available:
    roads, waterways, parcels, townland boundaries, building footprints.
  - Helps water/roads, but must be checked because 1820s layouts changed.

## Prototype Artifacts

Current dependency-free prototype:

```sh
python3 docs/graphics-v2/scripts/prototype_map_controls.py \
  --input docs/graphics-v2/grove-map-target-site-crop.png \
  --out-dir docs/graphics-v2/pipeline-experiments \
  --prefix grove-target-v3
```

Generated controls:

- `pipeline-experiments/grove-target-v3-ink-mask.png`
- `pipeline-experiments/grove-target-v3-semantic-mask.png`
- `pipeline-experiments/grove-target-v3-oblique-raw-warp.png`
- `pipeline-experiments/grove-target-v3-extruded-blockout.png`
- `pipeline-experiments/grove-target-v4-oblique-ink-warp.png`
- `pipeline-experiments/grove-target-v4-linework-control.png`

The prototype is deliberately crude. Its false positives are useful evidence:
simple thresholded connected components confuse text, tree symbols, road dots,
and building marks. Production needs text/symbol suppression and better feature
classification before the control render is trusted.

## Clean-Context Image Experiments

All experiments used fresh subagents, no thread history, no per-location hints,
one image-generation call each, and saved the exact prompt beside the output.

| ID | Control Inputs | Output | Result | Notes |
| --- | --- | --- | --- | --- |
| A | raw map + full style reference | `pipeline-experiments/idea-a-map-only.png` | Best source fidelity; pass with caveat | Most accurate result by far against the source map crop. The raw generator output contained a bottom-edge stream-like artifact that was excluded by crop, so this still needs automated artifact checks. |
| B | raw map + oblique raw-map warp + full style reference | `pipeline-experiments/idea-b-oblique-warp.png` | Fail | Invented water/bridge and chapel/churchyard cues. A raw geometric warp alone anchors perspective poorly and preserves tempting map/text artifacts. |
| C | raw map + semantic mask + full style reference | `pipeline-experiments/idea-c-semantic-mask.png` | Pass | Good balance. The mask seems to reduce wild inventions without forcing every bad detection into a building volume. Needs better extraction. |
| D | raw map + extruded blockout + full style reference | `pipeline-experiments/idea-d-extruded-blockout.png` | Pass with caveat | Good scale/facade control, but false building detections were persuasive. A bad blockout becomes bad geometry in the plate. |
| E | raw map + oblique cleaned-ink warp + full style reference | `pipeline-experiments/idea-e-oblique-ink-warp.png` | Fail | Imported chapel/cemetery motifs from the full style reference. Style references should avoid distinctive landmarks. |
| F | raw map + linework-only control + full style reference | `pipeline-experiments/idea-f-linework-control.png` | Control-path pass, lower source fidelity | Useful for stabilizing north-up geometry and avoiding church/water inventions, but less faithful than Cycle A. It over-rendered thin lines as substantial walls and changed the site read too much. |
| G | second raw map crop + full style reference | `pipeline-experiments/idea-g-raw-map-control-02.png` | Fail | Repeated the Cycle A method on another crop. Produced an unsupported church/churchyard and water. Likely cause: full-scene style-reference semantic leakage amplified by ambiguous estate/building/enclosure geometry. |

## Current Read

The most important metric is source-map fidelity, not the apparent stability of
the control path. Cycle A, using the raw map directly with the generic prompt
and style reference, was the most accurate by far. The useful pipeline should
therefore keep the historic map crop as the primary generation evidence.

Control layers are still useful, but as secondary aids and verification targets
until they consistently improve source fidelity. Heavy blockouts are powerful
only when extraction is trustworthy; otherwise they amplify mistakes.
Lightweight linework control is safer than extruded blockouts, but Cycle F shows
that even linework can over-assert walls and change the site read.

The full illustrated notebook image is a risky style reference because it
contains recognizable chapel/churchyard semantics. For future one-shot tests,
prefer cropped style references that show only:

- road/yard texture,
- wall/hedge/tree rendering,
- roof/facade rendering,
- watercolor/ink surface treatment.

Do not include a full scene with named landmarks when the model should infer
layout exclusively from the source map/control images.

Cycle G confirms this risk. The Beechwood crop did not receive per-location
hints, but the full style reference's church/churchyard composition appears to
have been imported as content. See
`pipeline-experiments/beechwood-church-leak-analysis.md`.

## Next Clean Pipeline Candidate

1. Start with a target-site map crop derived from fixed plate scale.
2. Generate ignore masks for text, numbers, survey marks, and app overlays.
3. Generate from the raw map crop as the primary layout reference.
4. Use only style crops, not a full scene, for visual style.
5. Generate a linework-only control image from cleaned ink for checking and
   optional soft conditioning.
6. Generate a semantic mask with conservative classes:
   - road/track corridors,
   - boundary lines,
   - building candidates,
   - tree/planting candidates,
   - water candidates,
   - ignore regions.
7. Use the semantic mask as a soft class hint only when it improves fidelity
   over raw-map-only generation.
8. Add building extrusions only after building extraction passes a confidence
   threshold, or render low-opacity footprint hints rather than full boxes.
9. Verify output against the source map and control layers; fail attractive
   renders that move roads, buildings, orchards, field boundaries, or exits.

The next experiment should compare:

- raw map + style crops,
- raw map + style crops + linework as soft secondary control,
- raw map + style crops + semantic mask as soft secondary control,
- raw map + style crops + post-generation control-layer verification only.
