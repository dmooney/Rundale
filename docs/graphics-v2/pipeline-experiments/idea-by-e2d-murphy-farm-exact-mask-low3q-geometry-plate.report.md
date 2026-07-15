# Idea BY E2d Murphy's Farm Exact-Mask Low 3/4 Geometry Plate Report

Purpose: deterministic geometry-match target for Murphy's Farm before any
style pass.

Inputs:

- `idea-by-e1g-murphy-farm-exact-local-mask-overhead-control.png` — accepted
  overhead topology control, with only the farmstead-local building mask and no
  global letter/number false positives.

Output:

- `idea-by-e2d-murphy-farm-exact-mask-low3q-geometry-plate.png`.
- `idea-by-e2d-murphy-farm-exact-mask-low3q-geometry-plate-annotated.png`.
- `../cartographic-comparisons/by-murphy-e2d-geometry-match-comparison.png`.

Method:

- Projected E1g into a low 3/4 ground plane with `y_squash = 0.58`.
- Extracted exactly three connected building-mask components from E1g.
- Extruded those exact mask silhouettes into simple placeholder low 3/4
  volumes.
- Preserved the west peat-bog control tint and source linework as projected
  control context.
- Added no imagegen, no style pass, no new road, no yard fill, and no garden
  interpretation.

Audit:

- Pass: preserves exactly three farmstead building components from the accepted
  E1g control.
- Pass: keeps the west/left peat-bog candidate area.
- Pass: keeps the diagonal source linework as faint projected source context
  rather than a road, driveway, or path.
- Pass: avoids the E2a/E2c failure modes of invented farm roads, broad yards,
  Y-roads, loop driveways, gardens, and scenic compound composition.
- Pass with caveat: the plate is schematic and not final art. It is intended as
  a hard geometry target for a future style/render pass.

Disposition: current geometry-match target for Murphy's Farm. Future imagegen
or style passes should use E2d as the geometry authority and should be rejected
if they regularize the building masks or add roads/yards that are not present
in E2d.
