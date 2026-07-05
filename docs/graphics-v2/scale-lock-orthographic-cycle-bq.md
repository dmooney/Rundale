# Scale-Lock Orthographic Cycle BQ

## Purpose

Cycle BQ responds to the user's correction after BP: the camera angle and art
style are close, but the image is still not truly isomorphic because distant
trees shrink relative to near trees. That matters for gameplay because a player
or NPC sprite should not need y-dependent scaling while walking around a static
background plate.

The key refinement is that "grid correctness" now has two parts:

```text
parallel line families
  + constant object/sprite scale across the whole plate
```

BP mostly checked the first part. BQ adds the second.

## Inputs

- Edit target:
  `pipeline-experiments/idea-bp-e2-kilteevan-art-last-grid-style-tighten.png`
- Scale-audit overlay:
  `pipeline-experiments/idea-bq-bp-e2-scale-audit-overlay.png`
- Pure scale-lock reference:
  `pipeline-experiments/idea-bq-isomorphic-scale-lock-reference.png`
- Style target:
  `illustrated-parish-notebook.png`

## Outputs

| ID     | Image                                                                                                                            | Prompt                                                                        | Report                                                                        | Result                                                                                       |
| ------ | -------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------- | ----------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| Assets | `pipeline-experiments/idea-bq-isomorphic-scale-lock-reference.png`, `pipeline-experiments/idea-bq-bp-e2-scale-audit-overlay.png` | n/a                                                                           | `pipeline-experiments/idea-bq-scale-lock-assets.report.md`                    | Defines the new constant-scale audit                                                         |
| E1     | `pipeline-experiments/idea-bq-e1-kilteevan-scale-lock-orthographic.png`                                                          | `pipeline-experiments/idea-bq-e1-kilteevan-scale-lock-orthographic.prompt.md` | `pipeline-experiments/idea-bq-e1-kilteevan-scale-lock-orthographic.report.md` | Partial pass: fixes much of the tree miniaturization, but hardens vegetation/garden material |

Comparison plate:

- `cartographic-comparisons/bq-scale-lock-orthographic-comparison.png`

## Verdict

The user's critique was correct. A line-only isomorphic grid can pass while the
render still has perspective scale cues. BP E2's distant trees were smaller than
near trees, which means it was not suitable as a constant-scale gameplay plate.

BQ E1 proves that the model can respond to a constant-scale instruction: the
top/north trees are larger and no longer read primarily as background scenery.
However, the edit spends style and material budget. It makes vegetation denser,
garden rows cleaner, and boundaries harder. That makes BQ E1 useful as a grid
direction signal, not as a final art target.

## Current Recommendation

Make the scale-lock audit part of the pipeline before further art polish:

```text
source/control/map authority
  -> low-oblique projection cue
  -> constant-scale marker/grid audit
  -> imagegen correction only if scale drift is visible
  -> notebook art/style pass
  -> final constant-scale audit
```

Acceptance for future candidates:

- same-size sprite marker can be placed on near, middle, and top roads/yards
  without changing pixel scale,
- same-kind trees do not shrink merely because they are farther north/top-frame,
- top content remains playable map ground, not scenic background,
- no visible grid/marker artifacts survive in the final image,
- garden/wall/path semantics do not harden while fixing scale.
