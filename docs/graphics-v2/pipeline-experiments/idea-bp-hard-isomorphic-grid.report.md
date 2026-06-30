# Idea BP Hard Isomorphic Grid

Generated deterministically with Pillow. No imagegen used.

Purpose: provide a hard projection check for Cycle BP, where perspective is solved before the final art-style pass.

Grid design:
- native `1672x941` frame, matching the current Kilteevan render size,
- two shallow low-oblique ground-axis line families at slope `+/-0.325`,
- all lines in each family are perfectly parallel,
- no vanishing point, horizon, barrel/fisheye bend, or camera convergence,
- grid is a reference/check only and must not appear in final art.

Generated outputs:
- `idea-bp-hard-isomorphic-grid-reference.png`
- `idea-bp-bo-e2-hard-isomorphic-grid-check.png`
- `idea-bp-bn-e2-hard-isomorphic-grid-check.png`
