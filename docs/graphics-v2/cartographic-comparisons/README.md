# Latest Map Accuracy Comparisons

Created 2026-06-28 for judging the current Graphics V2 map-accuracy path.

These plates make the top-down/control to low-3/4 isomorphic transformation an
explicit review step. Read the original rows left to right:

```text
source historic map crop -> topology/control step -> final isomorphic render
```

Cycle BM rows add the lower-pitch cue as a separate column:

```text
source historic map crop -> topology/control step -> lower-pitch cue -> final isomorphic render
```

Cycle BN rows add the north-extended source window and compressed camera cue:

```text
previous render -> north-extended source window -> compressed map cue -> blended cue -> final render
```

The source map remains the highest authority for feature existence and
orientation. The middle image is an intermediate topology aid or target, not
source truth.

| Plate                                    | Source map                                                        | Topology/control step                                                 | Final render                                                               |
| ---------------------------------------- | ----------------------------------------------------------------- | --------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| `beechwood-bj-source-control-render.png` | `../map-sources/beechwood-map-crop-control-02.png`                | `../pipeline-experiments/idea-m-beechwood-admin-topdown-cleaned.png`  | `../pipeline-experiments/idea-bj-beechwood-q-notebook-repaint.png`         |
| `grove-bh-source-target-render.png`      | `../grove-map-target-site-crop.png`                               | `../pipeline-experiments/idea-a-map-only.png`                         | `../pipeline-experiments/idea-bh-grove-bg-upper-structure-repair.png`      |
| `kilteevan-az-source-control-render.png` | `../pipeline-experiments/idea-ar-kilteevan-playable-map-crop.png` | `../pipeline-experiments/idea-at-kilteevan-tight-topdown-cleaned.png` | `../pipeline-experiments/idea-az-kilteevan-ay-low-camera-refine.png`       |
| `kilteevan-ba-source-control-render.png` | `../pipeline-experiments/idea-ar-kilteevan-playable-map-crop.png` | `../pipeline-experiments/idea-at-kilteevan-tight-topdown-cleaned.png` | `../pipeline-experiments/idea-ba-kilteevan-fresh-map-control-notebook.png` |

`latest-map-accuracy-contact-sheet.png` contains all four rows in one image.

Cycle BM transform-calibration comparisons:

- `bm-isomorphic-transform-matrix-contact-sheet.png` — all six BM candidates.
- `bm-e1-kilteevan-70-y048-comparison.png`
- `bm-e2-kilteevan-55-y040-comparison.png`
- `bm-e3-kilteevan-z17-y040-comparison.png`
- `bm-e4-kilteevan-path-wall-comparison.png`
- `bm-e5-beechwood-y040-comparison.png`
- `bm-e6-grove-y040-comparison.png`

Cycle BN north-extended low-camera comparisons:

- `bn-incremental-low-camera-contact-sheet.png` — E1/E2 rows with prior render,
  source window, compressed cue, blended cue, and final render.
- `bn-e1-20deg-incremental-comparison.png`
- `bn-e2-10deg-incremental-comparison.png`

Cycle BO orthographic-rectification comparison:

- `bo-orthographic-rectification-comparison.png` — BN E2 source draft, BO E1
  hard rectification, and BO E2 soft rectification.

Cycle BP art-last/grid-lock comparison:

- `bp-art-last-grid-locked-comparison.png` — BO E2 perspective-first base,
  BP E1/E2 grid checks, BP E1/E2 renders, and the original notebook style
  target.

Cycle BQ scale-lock comparison:

- `bq-scale-lock-orthographic-comparison.png` — BP E2 render, BP E2
  constant-scale audit, pure scale-lock marker reference, BQ E1 render, BQ E1
  constant-scale audit, and the original notebook style target.

Cycle BR close concept-art comparison:

- `br-beechwood-close-concept-comparison.png` — Beechwood source/control,
  old close target with black-door issue, BR E1 close raised-camera render, BR
  E1 symbol audit, and the original notebook concept target.

Cycle BS door-height calibration comparison:

- `bs-door-height-calibration-comparison.png` — original concept-art door-height
  crop, BR E1 close crop, and BS E1 calibrated crop shown at matching pixel
  crop size.
- `bs-e1-e2-concept-art-comparison.png` — original concept-art environment and
  door/facade crops beside BS E1 and BS E2 full/detail crops.

Cycle BT concept-realism weathering/clutter comparison:

- `bt-weathering-clutter-comparison.png` — concept art, BS E2 baseline, and BT
  E1/E2/E3 weathering/clutter/irregularity passes with full and detail crops.

Cycle BU concept-realism convergence comparison:

- `bu-concept-realism-convergence-comparison.png` — concept art, BT E2 clutter
  base, BU E1 hybrid, and BU E2 final tighten with full and facade/road/garden
  detail crops.

Cycle BV Grove reproducible-pipeline comparison:

- `bv-grove-reproducible-pipeline-comparison.png` — Grove map, Grove core
  control, BU E2 style target, BV E1 direct pipeline render, and BV E2 bounded
  tighten with full and detail crops.

Cycle BW Grove dry-stone wall comparison:

- `bw-grove-dry-stone-wall-comparison.png` — BV E2 baseline beside BW E1 and
  BW E2/BW E3, with full images and wall-detail crops for auditing the
  rectangular blockwork / bead-chain failure mode.
- `bw-e4-real-reference-boundary-comparison.png` — real dry-stone wall
  reference beside BV E2, BW E4, and BW E3; shows that the web reference helps
  material language only slightly unless the prompt also uses a regional
  hedgerow/bank/ditch prior.

Cycle BX Murphy's Farm pipeline steps:

- `bx-murphy-farm-pipeline-steps.png` — source map crop, deterministic
  soft-planting control, oblique camera cue, BU E2 style target, door-fixed
  references, boundary reference, direct E1 Murphy render, and preferred E2
  bounded roof/boundary fix. The source west-side texture is labeled as a peat
  bog candidate.

Cycle BY Murphy's Farm geometry-match comparisons:

- `by-murphy-e2d-geometry-match-comparison.png` — source map, accepted E1g
  overhead control, and deterministic E2d low 3/4 geometry target.
- `by-murphy-e2e-geometry-render-comparison.png` — source map, E1g, E2d, and
  first actual E2e geometry render.
- `by-murphy-e2f-shape-lock-comparison.png` — source map, E1g, E2d, E2e, and
  E2f shape-lock refinement. E2f is the geometry-preferred render.
- `by-murphy-e3-style-accuracy-comparison.png` — source map, E2d, E2f, BU E2
  style target, E3a, E3b, E3c, and the E3c door audit crop. E3c is the current
  preferred Murphy accuracy+style render.

Cycle BZ Grove subagent-gated proof comparison:

- `bz-grove-subagent-pipeline-proof-comparison.png` — Grove source crop,
  literal topology control, camera-only oblique cue, BU E2 style target, and
  the single render-subagent output. The independent audit verdict is PASS WITH
  CAVEATS: major geometry, doors, style, and camera pass, but ambiguous
  boundaries become too much continuous stone walling.
