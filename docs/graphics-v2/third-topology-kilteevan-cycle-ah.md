# Third Topology Test - Cycle AH

Cycle AH tests whether the current direct map/control prompt family can
generalize beyond the Grove and Beechwood crops while keeping the original
illustrated parish-notebook style.

The test crop is data-derived from the repository's configured historic map
source, not from hand-authored layout notes. It uses the NLS Roscommon
1st-edition 6-inch XYZ tile source already configured in Parish:

`https://mapseries-tilesets.s3.amazonaws.com/os/roscommon1/{z}/{x}/{y}.png`

The crop center is the stored Kilteevan Village coordinate from
`mods/rundale/world.json`. The source map remains the primary layout/content
evidence; the map-reader notes are reproducible soft disambiguation from the
generic rubric.

## Source And Control Artifacts

| Artifact             | Path                                                                        | Notes                                                                                                       |
| -------------------- | --------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| Source crop report   | `pipeline-experiments/idea-ah-kilteevan-z17-map-crop.report.md`             | Documents tile source, zoom, tile neighborhood, crop center, and crop origin.                               |
| Source mosaic        | `pipeline-experiments/idea-ah-kilteevan-z17-mosaic.png`                     | 3x3 fetched NLS tile mosaic at z17.                                                                         |
| Source crop          | `map-sources/kilteevan-z17-map-crop.png`                                    | North-up 16:9 crop used as the layout authority.                                                            |
| Map-reader notes     | `pipeline-experiments/idea-ah-kilteevan-map-reader-notes.md`                | Clean-context, generic-rubric, confidence-graded observations.                                              |
| Oblique pitch cue    | `pipeline-experiments/idea-ah-kilteevan-third-control-oblique-raw-warp.png` | Camera cue only, not source truth.                                                                          |
| Linework control     | `pipeline-experiments/idea-ah-kilteevan-third-control-linework-control.png` | Useful for rough line placement only.                                                                       |
| Semantic mask        | `pipeline-experiments/idea-ah-kilteevan-third-control-semantic-mask.png`    | Not recommended as content authority; over-detects symbols and misses building footprints.                  |
| Control report       | `pipeline-experiments/idea-ah-kilteevan-third-control-control-report.md`    | Notes the heuristic detector found zero building-like components.                                           |
| Direct render        | `pipeline-experiments/idea-ah-kilteevan-third-topology-direct.png`          | First third-topology render using source crop, map-reader notes, oblique cue, and cleaned style references. |
| Direct render prompt | `pipeline-experiments/idea-ah-kilteevan-third-topology-direct.prompt.md`    | Exact prompt used by the clean-context imagegen worker.                                                     |
| Direct render audit  | `pipeline-experiments/idea-ah-kilteevan-third-topology-direct.report.md`    | Worker audit of topology, admin-boundary handling, doors, style, and hard negatives.                        |

## Important Control Caveat

The prototype control script produced a useful oblique raw-map pitch cue, but
its semantic/building extraction is not reliable on this crop. It detected zero
building-like components and classified many tree/symbol marks as small
symbols. Therefore AH uses:

- the historic map crop as the primary layout/content authority,
- the map-reader note as confidence-graded soft disambiguation,
- the oblique raw warp only as camera-pitch cue,
- cleaned single-building style crops and material swatches only as style cues.

It does not use the semantic mask or extruded blockout as building truth.

## Audit Questions

- Does the render preserve the third crop's distinct topology: central road
  frontage cluster, upper enclosed compound, center-right planted enclosure,
  broad lower lane, thin field/yard boundaries, and northeastern tree/scrub
  mass?
- Does it ignore the bold diagonal dotted administrative/survey boundary and
  the curving pecked western line instead of turning them into hedges, walls,
  paths, tree rows, or crop rows?
- Does it avoid importing churches, graveyards, water, bridges, shops, people,
  carts, livestock, labels, signs, smoke, and chimneys from style references or
  project lore?
- Does every visible playable building facade have a readable door or
  threshold?
- Does the output feel like the original illustrated parish notebook rather
  than a clean strategy-board tile or high survey view?

## Result

AH is useful but not clean.

The direct render preserves the most important third-crop topology better than
expected: the central road-frontage building cluster remains multiple
buildings, the upper enclosed compound stays separate, the center-right planted
enclosure is legible, the broad lower lane and right-side lane remain coherent,
and the scene avoids copied labels, church, graveyard, water, bridge, shop,
people, animals, carts, smoke, and UI.

The failures are also important:

- The render appears to convert at least one likely dotted/pecked
  administrative or survey line into a substantial stone wall. It does not copy
  the dotted line literally, but it still materializes non-physical linework as
  in-world walling.
- Field and yard divisions are over-regularized into continuous stone walls,
  making the plate cleaner and more strategy-board-like than the source and the
  original notebook style warrant.
- The main slate-roofed building has a visible chimney-like stack despite the
  no-chimney/no-smoke hard negative.
- Door readability is mostly good, but one upper-compound small building has a
  weaker doorway from this camera angle.

Treat AH as a strong topology-generalization signal and a concrete negative
signal for boundary suppression/chimney suppression. It should not be promoted
to the cleaned visual target set.

## Recommendation

The next direct cycle should keep the same third-crop evidence stack but change
the render strategy:

- Make administrative/survey boundaries a positive omission rule: if marked
  non-physical, leave no continuous trace, not even a softened wall or hedge.
- Reduce wall conversion pressure for single thin field lines; use softer hedge,
  ditch, low overgrown boundary, grass color break, or no visible feature when
  confidence is low.
- Add a stricter roof audit phrase: no chimneys, no ridge stacks, no roof
  posts, no wall-top columns, no vertical masonry protrusions.
- Consider a bounded cleanup pass on AH only for diagnosing whether the chimney
  and over-wall issues can be repaired without topology drift, but keep that
  separate from one-shot/direct recipe evidence.

Cycle AI tested stronger prompt-only boundary/roof language on the same crop.
It fixed the chimney/roof-protrusion issue but still failed administrative
boundary suppression, which points toward a pre-cleaned physical-linework
control rather than more negative prompt text alone.
