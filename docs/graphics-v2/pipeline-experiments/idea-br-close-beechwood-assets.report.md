# BR Close Beechwood Assets Report

## Purpose

Cycle BR changes the requirements after the BQ scale-lock failure: rather than
forcing strict isomorphic projection, it tests whether a much smaller playable
area and slightly raised camera can recover more of the original concept-art
detail while keeping scale problems easy to see with local audit symbols.

## Assets

- `idea-br-beechwood-close-map-source.png` — tighter Beechwood source crop.
- `idea-br-beechwood-close-control.png` — tighter topology/control crop.
- `idea-br-beechwood-close-style-target-from-z.png` — old close visual target,
  kept as a caution because it has black doorway voids.
- `idea-br-beechwood-close-symbol-reference.png` — constant-size comparison
  symbols.
- `idea-br-beechwood-close-symbol-overlay.png` — symbol overlay on the old close
  target, showing why the old target was not sufficient.

## Notes

The old Beechwood Z close crop has good material density but bad door behavior:
several openings read as black voids. BR therefore uses the door-fixed
single-house crops as the door authority in the render prompt and keeps the Z
crop as a visual caution, not as the door model.
