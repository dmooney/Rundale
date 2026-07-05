# Compound/Cluster Crop Pair - Cycles X/Y

This note compares the Beechwood Cycle X compound-focused plate with the Grove
Cycle Y cluster-focused companion test.

The purpose is to verify that the current best Beechwood result is not just a
one-off. Both tests use the same principle: choose a smaller local topology
crop before the final render, generate a deterministic oblique pitch cue, then
ask for a notebook-style low 3/4 orthographic plate while preserving the crop's
building, yard, road, wall, garden, and tree relationships.

## Outputs

| Site        | Output                                                                  | Prompt                                                                        | Report                                                                        | Result                                    |
| ----------- | ----------------------------------------------------------------------- | ----------------------------------------------------------------------------- | ----------------------------------------------------------------------------- | ----------------------------------------- |
| Beechwood X | `pipeline-experiments/idea-x-beechwood-compound-focused-low-camera.png` | `pipeline-experiments/idea-x-beechwood-compound-focused-low-camera.prompt.md` | `pipeline-experiments/idea-x-beechwood-compound-focused-low-camera.report.md` | Best Beechwood notebook-scale pass so far |
| Grove Y     | `pipeline-experiments/idea-y-grove-cluster-focused-low-camera.png`      | `pipeline-experiments/idea-y-grove-cluster-focused-low-camera.prompt.md`      | `pipeline-experiments/idea-y-grove-cluster-focused-low-camera.report.md`      | Paired pass with caveats                  |

## Control Artifacts

Beechwood X:

- `pipeline-experiments/idea-x-beechwood-compound-focused-control.png`
- `pipeline-experiments/idea-x-beechwood-w-compound-focused-reference.png`
- `pipeline-experiments/idea-x-beechwood-compound-focused-control-oblique-raw-warp.png`

Grove Y:

- `pipeline-experiments/idea-y-grove-cluster-focused-control.png`
- `pipeline-experiments/idea-y-grove-u-style-reference-crop.png`
- `pipeline-experiments/idea-y-grove-cluster-focused-control-oblique-raw-warp.png`

## Working Hypothesis

The original illustrated parish notebook sample gets much of its appeal from
close playable scale: large readable facades, visible thresholds, muddy ground,
rough walls, and hand-painted texture density. Prompt wording alone cannot
recover that if the source/control crop covers too much ground. The crop must
be chosen from the desired plate scale first.

## Paired Audit

Beechwood X is the stronger camera/style match: it is closer, more facade-heavy,
more thatch-forward, and more like the original notebook sample's dense
hand-painted environment. It keeps the Beechwood compound connected around its
inner courtyard, solving the Cycle U topology failure.

Grove Y is a useful generalization check because it preserves a different
topology: multiple separated buildings around a working yard, not a connected
compound. It keeps the garden compound, yard, road exits, wall edges, and
building separation from its control crop. It is a little higher and more
roof-heavy than Beechwood X, and its garden walls/planting beds still read
regular, but it shares the same ink/watercolor material language and close
playable scale.

Together, X/Y support the crop-scale hypothesis. The best current recipe is not
"wide control plus stronger low-camera wording"; it is "choose a smaller local
map/control crop first, then render that crop with topology locked and notebook
style references."

## Remaining Risks

- X has one right-side exterior opening that can read more like a window than a
  clear playable doorway if that side face is intended to be navigable.
- Y loses the tiny north-of-garden outbuilding from the control crop, likely
  because it falls at the top crop edge.
- Y is slightly higher/roofier than X.
- Both plates still regularize planted plots and stone walls more than the
  rough original notebook sample.
- The style pair is good enough for research direction, but the pipeline still
  needs a batch test on several unrelated crops before calling it reproducible
  for ~100 locations.
