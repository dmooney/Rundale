# visual-pub-prop-kit-m15

M15 brings Darcy's Pub into the same raster sprite-compositor discipline as
Kilteevan and The Crossroads. The player experience should remain a warm,
playable pixel-art interior, but the scene should carry visible detail through
small reusable PNG kit atoms instead of only broad full-stage layers.

## Scope

- Derive small transparent PNG kit atoms from existing Darcy's Pub art.
- Register pub kit assets and repeated layers in `mods/rundale/scenes.json`.
- Use at least vessel/tabletop, barrel/wood, and warm light/fire-detail
  families.
- Extend the atom auditor so it verifies all three playable-slice scenes.
- Preserve the playable route: Kilteevan Village -> The Crossroads ->
  Darcy's Pub.

## Non-goals

- No SVG placeholders.
- No full-frame replacement render.
- No procedural interior tile map yet.
- No claim that these kits are final production sprite sheets.

## Risk

Interior prop repetition is easier to notice than mud or foliage. The proof
should favor small crops, subtle opacity, varied scale/flip, and placements
that reinforce existing shelves, counters, hearth, and tables rather than
adding new impossible objects.
