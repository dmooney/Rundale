# AE Roof-Nub Cleanup - Cycle AF

Cycle AF is a tiny defect cleanup on the AE crop-scale direct renders. AE gave
the strongest direct-control camera/scale evidence so far, but both plates had
small chimney-like roof nubs under strict review.

AF is not a new generation recipe. It uses the AE plate as the edit target and
asks for the smallest possible roof-nub cleanup while preserving topology,
style, crop, doors, thresholds, roads, yards, walls, gates, gardens, and tree
masses.

Important correction: AF does not fully pass the door/threshold audit. The
Beechwood AF foreground thatched cottage at lower right has a blank visible
wall with no readable door or threshold. Treat AF as a successful roof-nub
cleanup, not as a clean final target.

## Outputs

| Site | Output | Prompt | Report | Result |
| --- | --- | --- | --- | --- |
| Beechwood AF | `pipeline-experiments/idea-af-beechwood-ae-roof-nub-cleanup.png` | `pipeline-experiments/idea-af-beechwood-ae-roof-nub-cleanup.prompt.md` | `pipeline-experiments/idea-af-beechwood-ae-roof-nub-cleanup.report.md` | Roof cleanup pass; fails foreground cottage door audit |
| Grove AF | `pipeline-experiments/idea-af-grove-ae-roof-nub-cleanup.png` | `pipeline-experiments/idea-af-grove-ae-roof-nub-cleanup.prompt.md` | `pipeline-experiments/idea-af-grove-ae-roof-nub-cleanup.report.md` | Surgical cleanup pass |

## Audit Questions

- Did AF remove or flatten chimney-like roof nubs without introducing smoke,
  vents, pipes, or replacement roof clutter?
- Did it preserve AE's crop, topology, building count, roads, walls, gates,
  yards, gardens, doors, and thresholds?
- Did it avoid broad re-rendering or style drift?
- If AF succeeds, should AF become the best visual target while AE remains the
  best direct-control recipe evidence?

## Result

AF succeeded as a tiny roof-nub cleanup pass, but failed the stricter
all-visible-buildings door audit.

Beechwood AF preserves AE's crop, connected compound, building count, roads,
walls, gates, garden/enclosure context, ink, watercolor, and palette. The
chimney-like projection on the upper-left main roofline is flattened into
slate/moss roof texture and no smoke, stack, vent, pipe, or wall-attached
chimney remains there. However, the lower-right foreground thatched cottage has
no readable doorway on its visible wall, so Beechwood AF is not door-clean.

Grove AF preserves AE's crop, separate-building yard topology, road curve,
garden/enclosure context, gates, trees, doors, thresholds, ink, watercolor, and
palette. The small roof-edge nub near the thatched building is visually
flattened into continuous roof texture with no obvious vent, pipe, stack, or
smoke.

AF is no longer the current clean visual target pair. AE remains the better
evidence for the scalable direct-control recipe because it is generated
directly from core controls without a prior rendered edit target. AF still
shows that a small, bounded cleanup can remove residual roof-nub defects
without paying a topology drift cost, but a follow-up door/threshold repair is
needed before using the Beechwood plate as a style/target reference.

## Recommendation

Use this split going forward:

- **AE** for evaluating and improving the one-shot/direct-control pipeline.
- **AF** only as roof-nub cleanup evidence, not as a fully clean visual
  reference pair.

The next confidence gain should come from applying the AE core-crop method to
another unrelated topology crop, then allowing at most one tiny concrete cleanup
like AF. Avoid broad polish loops.
