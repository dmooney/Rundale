# Art-Last Grid-Check Cycle BP

## Purpose

Cycle BP tests the user's hypothesis after BO: the art style broke down because
the pipeline asked the final step to do too much at once. Instead of lowering
camera, rectifying projection, and restoring notebook style in one pass, BP
holds projection with a hard isomorphic grid and performs style recovery last.

The tested order is:

```text
BO E2 rectified low-oblique plate
  -> hard isomorphic grid/check constraint
  -> art-last notebook repaint
  -> grid-check audit
```

This is not a clean one-shot recipe because it starts from prior generated
plates. It is an ordering experiment.

## Inputs

- Perspective/content base:
  `pipeline-experiments/idea-bo-e2-kilteevan-bn-e2-soft-orthographic-rectify.png`
- Hard grid reference:
  `pipeline-experiments/idea-bp-hard-isomorphic-grid-reference.png`
- E1 grid audit:
  `pipeline-experiments/idea-bp-e1-hard-isomorphic-grid-check.png`
- E2 grid audit:
  `pipeline-experiments/idea-bp-e2-hard-isomorphic-grid-check.png`
- Style target:
  `illustrated-parish-notebook.png`
- Door/material references:
  `style-crops/illustrated-style-low-camera-slate-single-house-door-fixed.png`
  and
  `style-crops/illustrated-style-low-camera-thatched-single-house-door-fixed.png`

## Outputs

| ID  | Image                                                                       | Prompt                                                                            | Report                                                                            | Result                                                                           |
| --- | --------------------------------------------------------------------------- | --------------------------------------------------------------------------------- | --------------------------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| E1  | `pipeline-experiments/idea-bp-e1-kilteevan-art-last-grid-locked.png`        | `pipeline-experiments/idea-bp-e1-kilteevan-art-last-grid-locked.prompt.md`        | `pipeline-experiments/idea-bp-e1-kilteevan-art-last-grid-locked.report.md`        | Strong geometry/order proof; still too sepia and diagrammatic                    |
| E2  | `pipeline-experiments/idea-bp-e2-kilteevan-art-last-grid-style-tighten.png` | `pipeline-experiments/idea-bp-e2-kilteevan-art-last-grid-style-tighten.prompt.md` | `pipeline-experiments/idea-bp-e2-kilteevan-art-last-grid-style-tighten.report.md` | Preferred BP visual target; stronger notebook wash while grid check still passes |

Comparison plate:

- `cartographic-comparisons/bp-art-last-grid-locked-comparison.png`

## Verdict

The order change works. BP E1 already improves over BO by keeping the
projection straighter while recovering more hand-inked notebook material. BP E2
is the better final candidate: it loosens the palette and watercolor texture
without losing the main road/building/garden layout or reintroducing fish-eye
distortion.

The hard grid is useful as an audit/control artifact, not as content. Both E1
and E2 hide the grid in the final image, and the grid-check overlays show the
important roof ridges, road edges, garden rows, and wall edges staying close to
the same low-oblique parallel families.

## Remaining Weakness

The garden/orchard block is still too regular. BP makes it more painterly, but
the rows and border still read more diagrammatic than the original notebook
sample. This is the same underlying wall/path/planting semantics issue exposed
by BM through BO; BP improves the style/order problem, not the material
classification problem.

## Current Recommendation

Use this order for the next recipe-level test:

```text
source/control/map authority
  -> deterministic low-oblique projection/grid cue
  -> geometry/topology draft
  -> conservative projection audit/repair if needed
  -> final notebook-style repaint constrained by the hard grid
```

Do not ask a single imagegen call to lower camera, rectify isomorphic
projection, preserve map topology, and recover the original notebook style at
full strength. Give the model the hard projection law first, then spend the
last pass on style.
