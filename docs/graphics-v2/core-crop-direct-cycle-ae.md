# Core-Crop Direct Control - Cycle AE

Cycle AE tests whether the remaining camera/detail gap is partly a crop-scale
problem. It uses smaller core topology controls than AC so the 16:9 output
spends more pixels on buildings, yards, thresholds, road edges, walls, gates,
and immediate garden context.

This is a direct-control test, not an edit of AC or AD. No previous rendered
plate is supplied as an image input. The only generated controls used are the
core top-down crop and its matching oblique cue.

## New Control Artifacts

| Site | Core control | Oblique cue | Notes |
| --- | --- | --- | --- |
| Beechwood | `pipeline-experiments/idea-ae-beechwood-core-control-v2.png` | `pipeline-experiments/idea-ae-beechwood-core-control-v2-oblique-raw-warp.png` | Wider replacement for the first too-tight crop; the compound remains readable but an outer edge is still intentionally off-frame. |
| Grove | `pipeline-experiments/idea-ae-grove-core-control.png` | `pipeline-experiments/idea-ae-grove-core-control-oblique-raw-warp.png` | Smaller crop around buildings, yard, road curve, and nearby garden/enclosure context. |

Earlier discarded Beechwood crop-scale artifacts:

- `pipeline-experiments/idea-ae-beechwood-core-control.png`
- `pipeline-experiments/idea-ae-beechwood-core-control-oblique-raw-warp.png`

They were too aggressive and cut the compound hard at the left edge. Keep them
only as a cautionary artifact, not as the preferred AE input.

## Outputs

| Site | Output | Prompt | Report | Result |
| --- | --- | --- | --- | --- |
| Beechwood AE | `pipeline-experiments/idea-ae-beechwood-core-direct-clean-style.png` | `pipeline-experiments/idea-ae-beechwood-core-direct-clean-style.prompt.md` | `pipeline-experiments/idea-ae-beechwood-core-direct-clean-style.report.md` | Strong crop-scale pass, chimney-like nub caveat |
| Grove AE | `pipeline-experiments/idea-ae-grove-core-direct-clean-style.png` | `pipeline-experiments/idea-ae-grove-core-direct-clean-style.prompt.md` | `pipeline-experiments/idea-ae-grove-core-direct-clean-style.report.md` | Strong crop-scale pass, tiny roof-nub caveat |

## Audit Questions

- Does the smaller core crop produce larger, more human-scale buildings and
  more readable facades/doors/thresholds than AC?
- Does AE stay direct-control and avoid prior rendered plates?
- Does it preserve the cropped topology without inventing omitted context?
- Does Beechwood's edge-cropped compound remain coherent, or does the crop harm
  topology too much?
- Does Grove retain separate buildings without turning the smaller crop into a
  generic farmstead?
- Does crop scale reduce garden-board regularity, or does it simply zoom in on
  the same regularity?

## Result

AE supports the crop-scale hypothesis.

Both outputs are direct-from-control and use no previous rendered plate. They
spend more of the image on buildings, doors, thresholds, mud, walls, gates, and
nearby garden edges than AC. Compared with AC, the camera reads lower and more
human-scale, facades are easier to stage against, and the plates feel less like
miniature survey boards.

Beechwood AE preserves the connected compound, yard/courtyard, nearby garden
enclosures, road exits, walls, and gates from the core crop. The edge-cropped
compound remains coherent. The main failure is a small roof detail on the slate
building that reads chimney-like under the no-chimney rule.

Grove AE preserves the separate-building topology, road curve, central working
yard, garden/enclosure block, gates, and field/wall edges from the core crop.
It is the best direct-control Grove for readable facades and playable scale.
It has a tiny square roof-edge detail near the thatched building that could be
read as a vent or roof nub under strict review.

## Recommendation

Use core-crop controls as the next direct-control baseline. Crop scale is now a
stronger lever than more style adjectives for the camera/facade problem.

The next repair should be tiny and concrete: remove chimney-like roof nubs from
the AE pair while preserving topology, crop, and style. If that repair works,
the AE repaired pair should become the best direct-control visual target. If it
drifts topology, prefer raw AE for the scalable recipe and keep AD as the best
visual repair reference.
