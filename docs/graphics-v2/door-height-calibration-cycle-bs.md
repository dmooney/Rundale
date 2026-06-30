# Door-Height Calibration Cycle BS

## Purpose

Cycle BS responds to the door-scale correction after BR. BR E1 recovered much
of the original notebook warmth and detail, but it also pushed the crop too
close: doors became larger than the original concept-art standard. BS uses the
original illustrated parish notebook's door height as the camera/scale target.

The goal is intentionally narrow:

- zoom out a bit from BR E1,
- keep the close concept-art branch alive,
- match the notebook's readable door height,
- preserve fitted plank doors on every visible walkable-facade opening.

## Setup

BS E1 is a bounded edit from BR E1. The prompt removes the earlier BR pressure
to zoom closer and replaces it with a concrete door-height calibration:

```text
Compared with BR E1, doors should be materially smaller, roughly about
two-thirds as tall, while still clearly readable as wooden doors with
thresholds. Use the original notebook doors as the standard.
```

It also keeps the strict door rule from the door-repair branch:

```text
Every visible person-sized opening on every walkable facade must contain a
fitted wooden plank door and a threshold/step.
```

## Outputs

| ID | Image | Prompt | Report | Result |
| --- | --- | --- | --- | --- |
| E1 | `pipeline-experiments/idea-bs-e1-beechwood-door-height-calibrated-concept.png` | `pipeline-experiments/idea-bs-e1-beechwood-door-height-calibrated-concept.prompt.md` | `pipeline-experiments/idea-bs-e1-beechwood-door-height-calibrated-concept.report.md` | Door-height pass with caveats |
| E2 | `pipeline-experiments/idea-bs-e2-beechwood-door-height-20pct-zoomout.png` | `pipeline-experiments/idea-bs-e2-beechwood-door-height-20pct-zoomout.prompt.md` | `pipeline-experiments/idea-bs-e2-beechwood-door-height-20pct-zoomout.report.md` | 20% wider view; scale improves, concept-art messiness softens |

Comparison plate:

- `cartographic-comparisons/bs-door-height-calibration-comparison.png`
- `cartographic-comparisons/bs-e1-e2-concept-art-comparison.png`

## Verdict

BS E1 is a useful correction to BR E1. It is still visually close to the
original notebook branch, but the doors no longer read as close-up facade-study
doors. The focused comparison plate shows the scale shift clearly: BR E1 was
too close, while BS E1 is much closer to the concept-art door-height target.

BS E2 answers the follow-up request to zoom out 20% more. It succeeds on scale
and keeps fitted doors readable, but the comparison against the original concept
art shows the cost: the wider frame is cleaner, more orderly, and a little more
survey-like than the messy village density of the notebook sample.

The pass should not be overread. It remains an edited visual target, not a
fresh direct-map recipe, and the garden/perimeter walls still look cleaner and
more physical than the original notebook sample.

## Recommendation

For the relaxed concept-art branch, use door height as the first camera/scale
gate:

```text
Reject if the main doors are much larger than the original notebook doors.
Reject if any visible person-sized opening is only a dark void.
```

This is a better scale standard than "zoom closer" or "lower camera" alone.
