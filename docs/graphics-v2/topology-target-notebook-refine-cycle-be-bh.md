# Topology Target Notebook Refinement Cycle BE-BH

## Purpose

Cycles BE-BH test what to do after BD improved doors and roofs but still
regularized roads and garden boundaries into a scenic composed junction.

The main question was whether the next step should:

1. remove generated controls and trust the raw/cleaned map more directly, or
2. use the most accurate rendered topology plate as a bounded edit target and
   repaint it toward the original notebook look.

## BE: Raw/Cleaned Map Only Retry

BE intentionally removed generated top-down and soft-planting controls. The
inputs were:

- tight original map crop,
- tight cleaned no-admin crop,
- deterministic oblique camera cue,
- original notebook sample as style only,
- clean slate and thatch single-building references,
- tree/field watercolor reference.

Output:

- `pipeline-experiments/idea-be-kilteevan-raw-map-notebook-no-topdown.png`
- `pipeline-experiments/idea-be-kilteevan-raw-map-notebook-no-topdown.prompt.md`
- `pipeline-experiments/idea-be-kilteevan-raw-map-notebook-no-topdown.report.md`

Result: useful negative. BE kept strong notebook style, readable facades, doors
on openings, and mostly good roof discipline, but it still regularized into a
scenic central road composition. It added yard/path-like connectors, made
garden/internal marks too wall-like, and leaked barrel/tub-like props.

Interpretation: removing generated controls does not solve the scenic-road
prior. The model still composes a picturesque crossroads when asked to infer a
finished scene directly.

## BF: Bounded Repaint From Cycle A

BF used `idea-a-map-only.png` as the edit target because Cycle A remains the
strongest source-fidelity topology read in the current files. It asked the
model to preserve Cycle A's roads, buildings, garden, field masses, walls,
gates, crop, and camera while repainting toward the original notebook sample
and applying the newer no-props, no-chimneys, and doors-on-openings rules.

Output:

- `pipeline-experiments/idea-bf-grove-a-topology-notebook-refine.png`
- `pipeline-experiments/idea-bf-grove-a-topology-notebook-refine.prompt.md`
- `pipeline-experiments/idea-bf-grove-a-topology-notebook-refine.report.md`

Result: strong visual improvement and better topology preservation than fresh
BE/BD. It removes props, improves notebook ink/watercolor, keeps the major road
and garden topology, keeps doors fitted into the clear openings, and avoids
chimneys/smoke/roof nubs.

Failure: BF over-cleaned one small roof-like structure along the upper garden
wall, flattening it into a gate/wall mark.

## BG: Structure-Preserving Repaint Retry

BG repeated BF with stricter wording: every roofed or roof-like built structure
from Cycle A must remain distinct unless unmistakably movable clutter.

Output:

- `pipeline-experiments/idea-bg-grove-a-structure-preserving-notebook-refine.png`
- `pipeline-experiments/idea-bg-grove-a-structure-preserving-notebook-refine.prompt.md`
- `pipeline-experiments/idea-bg-grove-a-structure-preserving-notebook-refine.report.md`

Result: stylistically strong, but not a real topology improvement over BF. It
still flattened the small upper garden-wall structure into a gate/wall detail.

Interpretation: once a prior edit target has over-cleaned an ambiguous small
structure, using it as a style/cleanup reference can anchor that loss. Global
wording alone was not enough to restore the feature.

## BH: Local Structure Repair

BH used BG as the edit target and provided two local references:

- `pipeline-experiments/idea-bh-grove-a-upper-garden-structure-reference.png`
  from Cycle A, showing the small roof-like structure,
- `pipeline-experiments/idea-bh-grove-map-upper-garden-reference.png` from the
  warped source map, showing a small dark rectangular mark in the same region.

Output:

- `pipeline-experiments/idea-bh-grove-bg-upper-structure-repair.png`
- `pipeline-experiments/idea-bh-grove-bg-upper-structure-repair.prompt.md`
- `pipeline-experiments/idea-bh-grove-bg-upper-structure-repair.report.md`

Result: best current visual target in this branch. The tiny upper garden-wall
outbuilding is restored; the global frame, roads, gardens, walls, fields, main
buildings, doors, no-prop cleanup, and no-chimney roof discipline remain
visually preserved.

This is still edit-target evidence, not one-shot recipe proof. Because the edit
uses imagegen without an explicit pixel mask, subtle full-frame repainting may
exist.

## Current Recommendation

Do not promote BE, BF, BG, or BH as one-shot recipe proof.

Promote the pipeline lesson:

```text
raw map / cleaned map
  -> source-faithful topology target (Cycle A-like or better)
  -> bounded notebook repaint preserving topology
  -> local repairs for concrete topology/door/roof failures
```

For production/batch work, the next research step should make the topology
target more deterministic or more auditable, then run bounded style transfer or
masked repairs. Fresh final renders from raw map plus text prompts are still too
vulnerable to scenic-road and garden-wall priors.

Keep the "doors on openings" wording from BD onward.
