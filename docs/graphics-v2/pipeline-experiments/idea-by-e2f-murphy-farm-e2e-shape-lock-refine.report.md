# Idea BY E2f Murphy's Farm E2e Shape-Lock Refine Report

Purpose: one targeted imagegen refinement of E2e to better match the E2d
three-building geometry.

Inputs:

- `idea-by-e2e-murphy-farm-e2d-exact-geometry-render.png` — edit target.
- `idea-by-e2d-murphy-farm-exact-mask-low3q-geometry-plate.png` — hard low
  3/4 geometry authority.
- `idea-by-e1g-murphy-farm-exact-local-mask-overhead-control.png` and
  `idea-bx-murphy-farm-z17-map-crop.png` — source/control veto references.

Output:

- `idea-by-e2f-murphy-farm-e2e-shape-lock-refine.png`.
- `../cartographic-comparisons/by-murphy-e2f-shape-lock-comparison.png`.

Audit:

- Pass: keeps exactly three buildings.
- Pass: keeps the farmstead compact and closer to the E2d footprint than E2e.
- Pass: preserves the west peat-bog field and faint source-map context.
- Pass: still avoids invented roads, driveways, gardens, walls, extra
  buildings, people, animals, carts, and smoke.
- Pass with caveat: the three masses remain more cottage-like than true
  map-symbol extrusions. This may be acceptable for the later style pass, but
  it is not a pixel-exact building-shape transfer.
- Caveat: the faint local control/field linework around the cluster is slightly
  more visible than in E2e, though it does not read as a finished physical
  enclosure.

Disposition: geometry-preferred rendered candidate for Murphy's Farm. E2f is
the current stopping point for the geometry-match goal; the next stage should
be style-last only if this geometry is accepted.
