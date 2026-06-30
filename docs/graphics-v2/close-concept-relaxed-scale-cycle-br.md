# Close Concept Relaxed-Scale Cycle BR

## Purpose

Cycle BR tests the user's revised direction after BQ: if strict isomorphic
projection fights the image model too hard, shrink the playable area, raise the
camera slightly, relax the firm isomorphic constraint, and try to recover the
original concept-art detail level at another location.

The chosen non-Kilteevan location is Beechwood because prior cycles already
identified a useful connected-compound topology and a close crop exposes the
door/facade/detail problem clearly.

## Setup

BR deliberately avoids the earlier wide-map failure mode:

- much smaller playable area,
- roads and garden edges can exit frame,
- no distant building row,
- no far-north scenic band,
- slightly raised camera compared with Beechwood Z,
- door-fixed style crops override old black-door visual targets,
- comparison symbols are added after generation as an audit overlay.

## Outputs

| ID | Image | Prompt | Report | Result |
| --- | --- | --- | --- | --- |
| Assets | `pipeline-experiments/idea-br-beechwood-close-*.png` | n/a | `pipeline-experiments/idea-br-close-beechwood-assets.report.md` | Tight Beechwood controls plus symbol references |
| E1 | `pipeline-experiments/idea-br-e1-beechwood-close-raised-camera-door-fixed-concept.png` | `pipeline-experiments/idea-br-e1-beechwood-close-raised-camera-door-fixed-concept.prompt.md` | `pipeline-experiments/idea-br-e1-beechwood-close-raised-camera-door-fixed-concept.report.md` | Current BR pass: closer, warmer, doors restored, relaxed scale |

Comparison plate:

- `cartographic-comparisons/br-beechwood-close-concept-comparison.png`

## Verdict

BR E1 is the strongest evidence so far for the relaxed requirement. It looks
closer to the original notebook concept art than the strict-grid branch because
the prompt no longer spends all its budget on orthographic correction. The
closer crop also helps: door and thatch detail occupy enough pixels to become
readable.

The important correction from the user's interruption held: the old Beechwood Z
crop had black doorway voids, but BR E1's visible person-sized openings read as
fitted plank doors with thresholds.

This does not make BR E1 a strict isomorphic runtime background. It is a visual
target for the alternative direction: close illustrated local plate, relaxed
scale, symbols available for audit, and runtime design still needs to decide
whether that relaxed scale is acceptable.

## Current Recommendation

Keep BR's requirement split:

```text
strict-isomorphic branch:
  deterministic/procedural projection, runtime-safe scale

concept-art branch:
  tiny local crop, slightly raised camera, relaxed scale,
  rich notebook texture, symbols overlaid for judgment
```

For concept-art exploration, BR E1 is the better direction than more prompt-only
isomorphic repair on the wide Kilteevan frame.
