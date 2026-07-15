# BQ Scale-Lock Audit Assets Report

## Purpose

Cycle BP's hard grid checked parallel line families, but it did not check
constant world scale. That missed the user's key issue: distant/top-of-frame
trees were smaller than near/bottom trees, which would force runtime sprites to
use an unknown y-dependent scale.

Cycle BQ adds scale-lock audit artifacts:

- `idea-bq-isomorphic-scale-lock-reference.png`
- `idea-bq-bp-e2-scale-audit-overlay.png`
- `idea-bq-bp-e2-scale-markers-only.png`

## What The Markers Mean

The green rings represent same-size tree/object crown checks. The magenta
standees represent same-size player sprite rulers. In a true orthographic /
isomorphic game plate, those markers should remain the same pixel size anywhere
on the map.

The overlay is diagnostic only. The final image must not contain the markers or
grid lines.

## Finding

BP E2 passes the parallel-line grid better than earlier experiments, but fails
the constant-scale audit. The top/north tree masses are painted as distant
background objects, not same-scale playable map objects.

Future grid checks should include both:

- parallel projection families for roads, roof ridges, walls, and garden rows,
- equal-size scale markers across near, middle, and far playable rows.
