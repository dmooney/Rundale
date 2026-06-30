# Orthographic Rectification Cycle BO

## Purpose

Cycle BO tests the user's follow-up hypothesis after BN: let the model create a
content-rich very-low-camera draft first, even if it has some fisheye/barrel
distortion, then run a final bounded pass that only rectifies the image back
into a usable low oblique orthographic/isomorphic game plate.

The hypothesis is that "lower the camera" and "stay strictly orthographic" may
be too much to solve in the same render. BO separates them:

```text
north-extended low-camera content render
  -> conservative orthographic/fisheye rectification pass
```

## Inputs

- Source/edit target: `pipeline-experiments/idea-bn-e2-kilteevan-north-10deg-incremental.png`
- E1 projection proof: direct rectification from BN E2.
- E2 retry: soft rectification from BN E2, with E1 used only as a projection
  reference and explicitly rejected as a content/style reference.

## Outputs

| ID | Image | Prompt | Report | Result |
| --- | --- | --- | --- | --- |
| E1 | `pipeline-experiments/idea-bo-e1-kilteevan-bn-e2-orthographic-rectify.png` | `pipeline-experiments/idea-bo-kilteevan-bn-e2-orthographic-rectify.prompt.md` | `pipeline-experiments/idea-bo-e1-kilteevan-bn-e2-orthographic-rectify.report.md` | Strong geometry proof; over-cleans and adds fence/wall hardness |
| E2 | `pipeline-experiments/idea-bo-e2-kilteevan-bn-e2-soft-orthographic-rectify.png` | `pipeline-experiments/idea-bo-e2-kilteevan-bn-e2-soft-orthographic-rectify.prompt.md` | `pipeline-experiments/idea-bo-e2-kilteevan-bn-e2-soft-orthographic-rectify.report.md` | Best BO candidate; softer rectification, lower distortion, less overbuild |

Comparison plate:

- `cartographic-comparisons/bo-orthographic-rectification-comparison.png`

## Verdict

The decomposition works. BO E1 proves the model can take BN E2's low-camera
draft and make it read more like a parallel-projection game plate. Roads,
building alignments, and the garden block become less bowed and less fisheye.

The caveat is that a broad "rectify into orthographic/isomorphic" instruction
invites the model to redraw the scene as a cleaner plan. BO E1 added too much
post-and-rail fence and hard boundary language.

BO E2 is the better candidate. It still reduces the lensy/barrel feel while
preserving more of BN E2's low camera, crop, source-backed north/background
content, and softer material treatment. It is not perfect: the garden/orchard
block remains hard-edged and fairly diagrammatic. But it is the best result of
this bounded two-render test.

## Current Recommendation

Keep this as the next camera pipeline shape:

```text
north-extended source/control
  -> low-camera content draft
  -> conservative soft orthographic rectification
```

The rectification prompt should avoid the words "clean plan" or anything that
sounds like redrawing. It should say "barrel correction only," "do not repaint,"
"do not add fences/walls," and "preserve soft ambiguity." Treat BO E2 as the
current candidate final-step target, with the remaining known weakness being
garden/wall material semantics.
