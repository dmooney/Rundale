# Incremental Low-Camera Cycle BN

## Purpose

Cycle BN follows the user-requested next experiment after Cycle BM: lower the
camera more aggressively, but give the model real far-north map evidence so the
top/background of the plate is sourced from the historic map instead of
invented scenery.

The target was incremental, not open-ended:

- first a 20-degree low-camera step from BM E4,
- then a 10-12 degree step, roughly 50% lower than the BM baseline,
- stop once the experiment brackets the camera signal.

## Deterministic Setup

The deterministic north-extension assets are documented in
`pipeline-experiments/idea-bn-north-extension-assets.report.md`.

- The Kilteevan z17 source was rebuilt as a north-extended mosaic covering tile
  rows `42306..42312`.
- The resulting source mosaic is `768x1792`.
- The active 55% playable core stays anchored near the southern end of the
  mosaic.
- Three cue windows were prepared:
  - 20 degrees: about `2x` the prior north/south extent,
  - 15 degrees: about `3x` the prior north/south extent,
  - 10 degrees: about `4x` the prior north/south extent.

The 15-degree cue was prepared but not rendered. E1 and E2 already bracketed
the useful signal, so spending a third imagegen call on the midpoint would have
been lower-value.

## Outputs

| ID  | Image                                                                   | Prompt                                                                        | Report                                                                        | Result                                                          |
| --- | ----------------------------------------------------------------------- | ----------------------------------------------------------------------------- | ----------------------------------------------------------------------------- | --------------------------------------------------------------- |
| E1  | `pipeline-experiments/idea-bn-e1-kilteevan-north-20deg-incremental.png` | `pipeline-experiments/idea-bn-e1-kilteevan-north-20deg-incremental.prompt.md` | `pipeline-experiments/idea-bn-e1-kilteevan-north-20deg-incremental.report.md` | Adds north-backed background, but still reads too high          |
| E2  | `pipeline-experiments/idea-bn-e2-kilteevan-north-10deg-incremental.png` | `pipeline-experiments/idea-bn-e2-kilteevan-north-10deg-incremental.prompt.md` | `pipeline-experiments/idea-bn-e2-kilteevan-north-10deg-incremental.report.md` | First strong 50%-lower camera signal; topology/semantics caveat |

Comparison plates live in `docs/graphics-v2/cartographic-comparisons/`:

- `bn-incremental-low-camera-contact-sheet.png`
- `bn-e1-20deg-incremental-comparison.png`
- `bn-e2-10deg-incremental-comparison.png`

Each row reads:

```text
previous render -> north-extended source window -> compressed map cue -> blended cue -> render
```

## Verdict

E2 confirms the user's diagnosis. If the overhead evidence only covers the
playable core, the model resists a truly lower camera or invents generic
background. When the source extends far to the north, the model can lower the
camera much more while still filling the top of the frame with source-backed
fields, roads, tree rows, and open land.

The camera signal passes:

- facades and doors are materially larger than BM E4 and BN E1,
- the main building reads as a foreground structure rather than a survey-board
  icon,
- the frame is closer to the original concept-art perspective while retaining
  orthographic/game-plate discipline,
- the far/top/north content is plausibly derived from the extended map window.

The result is not a final recipe. The lower camera spends accuracy budget on
feature semantics:

- garden edges and internal rows become more physical and wall-like,
- fence/wall marks become darker and more emphatic,
- the road/path/boundary distinction remains fragile at the lower angle.

## Current Recommendation

Treat BN E2 as the camera target proof, not as a complete production recipe.
The next pipeline step should keep the 10-degree / far-north-source idea, but
pair it with stronger upstream material controls for:

- open muddy roads and tracks,
- soft planting and garden rows,
- rare explicit wallable edges,
- no-trace or low-confidence treatment for ordinary parcel/admin/no-data
  linework.

Do not spend more imagegen calls on prompt-only "lower camera" variants until
that semantic control is improved. The camera problem is now mostly solved
enough to expose the boundary/material problem.
