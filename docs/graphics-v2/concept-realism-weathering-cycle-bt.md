# Concept Realism Weathering Cycle BT

## Purpose

Cycle BT tests how to move BS E2 toward the original illustrated parish
notebook's realism without changing BS E2's hard-won zoom, compound topology,
and fitted-door discipline.

The problem after BS E2 was clear: scale and doors were close, but the scene
still looked too clean, ordered, and estate-plan-like. BT tests three bounded
prompt directions:

- E1: surface weathering only,
- E2: sparse lived-in yard clutter,
- E3: irregular garden/walls/road geometry.

## Outputs

| ID  | Image                                                              | Prompt                                                                   | Report                                                                   | Result                                  |
| --- | ------------------------------------------------------------------ | ------------------------------------------------------------------------ | ------------------------------------------------------------------------ | --------------------------------------- |
| E1  | `pipeline-experiments/idea-bt-e1-bs-e2-surface-weathering.png`     | `pipeline-experiments/idea-bt-e1-bs-e2-surface-weathering.prompt.md`     | `pipeline-experiments/idea-bt-e1-bs-e2-surface-weathering.report.md`     | Safe but too conservative               |
| E2  | `pipeline-experiments/idea-bt-e2-bs-e2-lived-in-yard-clutter.png`  | `pipeline-experiments/idea-bt-e2-bs-e2-lived-in-yard-clutter.prompt.md`  | `pipeline-experiments/idea-bt-e2-bs-e2-lived-in-yard-clutter.report.md`  | Best single tested realism direction    |
| E3  | `pipeline-experiments/idea-bt-e3-bs-e2-irregular-garden-walls.png` | `pipeline-experiments/idea-bt-e3-bs-e2-irregular-garden-walls.prompt.md` | `pipeline-experiments/idea-bt-e3-bs-e2-irregular-garden-walls.report.md` | Best regularity fix, but too dark/heavy |

Comparison plate:

- `cartographic-comparisons/bt-weathering-clutter-comparison.png`

Recommended prompt direction:

- `pipeline-experiments/idea-bt-recommended-bs-e2-concept-realism-hybrid.prompt.md`

## Verdict

The winning direction is not pure weathering. It is a hybrid:

```text
BT E2 sparse practical clutter + BT E3 irregular garden/wall/road edges,
with an explicit cap on repeated buckets/barrels and an explicit lighter
watercolor value range than BT E3.
```

BT E2 is the best single tested image because it makes the place feel used
without overly darkening the plate or disrupting the compound. BT E3 provides
the important second ingredient: break the regular rows, tidy walls, and clean
road edges. A future combined render should use E2 as the baseline and borrow
only the irregularity controls from E3.

## Recommendation

Use the recommended hybrid prompt for the next render. Keep the pass bounded:
one combined render, then audit against the concept art for:

- lived-in realism,
- not too dark,
- no repeated bucket/barrel pattern,
- no new buildings/roads/landmarks,
- all visible walkable-facade openings still fitted with readable plank doors.
