# AO Roof Stub Cleanup Cycle AP

## Purpose

Cycle AO is the best direct-control signal so far for open-field boundary
restraint, but it introduces roof chimneys/stacks. Cycle AP is a bounded visual
cleanup of AO, not a direct recipe run: remove roof/stub artifacts while
preserving AO's open fields, map topology, doors, and notebook style.

## Input

- Edit target:
  `pipeline-experiments/idea-ao-kilteevan-open-fields-direct.png`

No original map/control images are used as layout authorities in AP because the
goal is not a fresh recipe test. AO remains the direct recipe evidence.

## Required Preservation

- open-field softness,
- broad lower road and road junction,
- central building cluster,
- upper compound,
- center-right planted garden/orchard,
- northeast scrub/tree mass,
- building footprints and roof materials,
- all visible doors and thresholds,
- north-up 3/4 orthographic/isomorphic camera,
- ink-and-watercolor notebook texture.

## Output

Output:

- `pipeline-experiments/idea-ap-kilteevan-ao-roof-stub-cleanup.png`
- `pipeline-experiments/idea-ap-kilteevan-ao-roof-stub-cleanup.prompt.md`
- `pipeline-experiments/idea-ap-kilteevan-ao-roof-stub-cleanup.report.md`

## Result

AP is not a clean pass. The edit preserved AO's overall layout and field
softness, but zoom inspection shows two visible roof artifacts remain:

- a small stack on the upper-compound slate-roof building,
- a prominent chimney on the lower-left slate-roof building.

Treat AP as a failed/partial cleanup despite its generated report claiming the
two artifacts were removed. The next retry should include zoom crops of the
exact failures and explicitly call out those two locations.

## Audit Questions

- Are all visible chimneys, stacks, roof nubs, wall stacks, vents, pipes, and
  protrusions removed?
- Did the edit preserve AO's open-field softness without reintroducing a
  stone-wall network?
- Did roads, buildings, gardens, and tree masses stay in place?
- Did all readable doors and thresholds remain readable?
- Did the illustrated notebook style remain consistent rather than becoming a
  smoothed or photorealistic patch?

## Audit Answers

- Roof artifacts: fail; two visible chimneys/stacks remain.
- AO topology/open fields: mostly preserved.
- Doors/thresholds: appear preserved at the inspected scale.
- Style: preserved, but the repair objective was not met.

## Cycle AP2 Retry

AP2 retries the bounded cleanup with zoom crops of the exact remaining defects:

- upper-compound slate-roof stack,
- lower-left slate-roof chimney.

Expected outputs:

- `pipeline-experiments/idea-ap2-kilteevan-ao-roof-stub-cleanup-retry.png`
- `pipeline-experiments/idea-ap2-kilteevan-ao-roof-stub-cleanup-retry.prompt.md`
- `pipeline-experiments/idea-ap2-kilteevan-ao-roof-stub-cleanup-retry.report.md`

## Cycle AP2 Result

AP2 is a useful improvement but not a fully verified clean pass. The lower-left
slate-roof chimney was removed and the foreground slate cottage still has a
readable door and threshold. Central roofs and doors remain readable.

The upper-compound slate roof still has a tiny dark ridge/edge mark that may
read as a residual roof nub at zoomed inspection. AP2 also appears globally
softened/repainted compared with AO/AP rather than being a perfectly local edit.
Treat it as a cleaned visual target with caveats, not as direct recipe evidence
and not as proof that the bounded cleanup prompt is reliable.

Additional audit note: subagent/imagegen reports can overclaim cleanup quality.
Always inspect the full plate plus focused zoom crops before accepting a repair,
and include every visible walkable building in the door/threshold audit.

## Cycle AP3 Result

AP3 retries AP2's remaining questionable upper-compound roof mark. It starts
from AP2, uses a focused crop of the upper-compound slate-roof building as the
defect reference, and writes:

- `pipeline-experiments/idea-ap3-kilteevan-ap2-upper-roof-nub-cleanup.png`
- `pipeline-experiments/idea-ap3-kilteevan-ap2-upper-roof-nub-cleanup.prompt.md`
- `pipeline-experiments/idea-ap3-kilteevan-ap2-upper-roof-nub-cleanup.report.md`

Main-agent zoom audit:

- Upper-compound roof: pass; no obvious isolated chimney/stub remains on the
  larger slate roof, and the doorway remains readable.
- Central cluster: pass; central doors/thresholds remain readable and roof
  marks read as slate/watercolor texture rather than chimneys.
- Foreground cluster: pass; the slate cottage has a clear dark door, and the
  thatched foreground building still reads as having a recessed doorway or
  threshold.
- Strict minimality: caveat; like AP2, AP3 subtly re-renders global texture and
  contrast, so it is not a pixel-local repair.

Treat AP3 as the best current cleaned AO visual target, but still not as direct
recipe evidence. AO remains the direct-control evidence for open-field boundary
restraint; AP3 is a downstream visual cleanup artifact.
