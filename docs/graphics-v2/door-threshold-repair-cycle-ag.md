# Door/Threshold Repair - Cycle AG

Cycle AG is a bounded correction to Beechwood AF after stricter visual audit.
AF fixed the chimney-like roof nubs, but the lower-right foreground thatched
cottage still had a blank visible wall with no readable entrance. That failure
makes AF unsafe as a visual target or style reference.

AG is not a new one-shot/direct-control recipe. It uses Beechwood AF as an edit
target and asks for the smallest possible doorway/threshold repair while
preserving the map-derived layout, crop, north-up isomorphic camera, roofs,
walls, roads, gates, gardens, and watercolor/ink style.

## Output

| Site | Output | Prompt | Report | Result |
| --- | --- | --- | --- | --- |
| Beechwood AG | `pipeline-experiments/idea-ag-beechwood-af-door-threshold-repair.png` | `pipeline-experiments/idea-ag-beechwood-af-door-threshold-repair.prompt.md` | `pipeline-experiments/idea-ag-beechwood-af-door-threshold-repair.report.md` | Door/threshold repair pass |

## Audit Questions

- Did the lower-right foreground thatched cottage gain a clear readable
  doorway/threshold?
- Do all visible standalone buildings and visible building wings have at least
  one readable door, doorway, gate opening, or threshold unless they are clearly
  ruins or wall segments?
- Did the edit preserve the Beechwood compound, garden plots, walls, gates,
  roads, roof shapes, building count, and north-up isomorphic camera?
- Did it avoid adding churches, chapels, water, bridges, people, animals, text,
  UI, smoke, fog, chimneys, vents, or chimney-like roof nubs?
- Did the edit stay bounded, or did the model repaint enough of the plate that
  it should be treated as a broader generative variant?

## Result

AG fixes the specific Beechwood AF audit failure. The lower-right foreground
thatched cottage now has a readable dark doorway and a small threshold/slab on
the visible wall. The main compound and visible wings still read as having
doorways, dark openings, gate openings, or thresholds.

The edit appears to preserve the crop, north-up isomorphic composition,
connected compound, building count, road/wall/gate structure, garden plots,
thatched cottage placement, and quiet notebook palette. It does not visibly add
churches, chapels, water, bridges, people, animals, labels, UI, smoke, fog, or
new chimneys.

The caveat is that the built-in image edit mildly repainted texture across the
whole plate rather than making a perfectly local inpaint. Use AG as the current
cleaned Beechwood visual target, but keep AE as the direct-control recipe
evidence and avoid presenting AG as one-shot proof.

## Recommendation

Use this split until the next direct-control cycle replaces it:

- **Beechwood AG** as the current cleaned Beechwood visual target.
- **Grove AF** as the current cleaned Grove visual target, pending the same
  strict all-visible-buildings doorway audit.
- **AE** as the scalable direct-control recipe evidence because it comes
  directly from core controls rather than from a prior rendered edit target.

Future audits must check every visible habitable building, including foreground
and edge cottages, not only the central compound or main house.
