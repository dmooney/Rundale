# visual-kilteevan-prop-kit-m14

M13 proved reusable physical prop kits at The Crossroads. M14 brings that same
discipline to the first scene players see: Kilteevan Village. The player
experience should not change mechanically; the launch scene should simply feel
more like a game screen built from reusable raster sprites instead of a single
illustrated backdrop with a few overlays.

## Scope

- Derive small transparent PNG atoms from the current Kilteevan pixel art.
- Add reusable kit assets and repeated Kilteevan layers to
  `mods/rundale/scenes.json`.
- Use at least wall, foliage, and terrain/road-detail families.
- Extend `parish/apps/visual/scripts/audit-scene-atoms.mjs` so the local audit
  verifies more than one scene and reports coverage per scene.
- Preserve the three-scene playable slice: Kilteevan Village -> The Crossroads
  -> Darcy's Pub.

## Non-goals

- No SVG placeholders.
- No full-frame replacement render.
- No procedural tile map yet.
- No claim that these are final production sprites; this is a stricter proof
  of compositing direction.

## Risk

Kilteevan is the launch scene, so repeated prop atoms are more likely to be
scrutinized. The safest approach is to use modest opacity, varied scale/flip,
and place kits where broad base atoms already justify extra pixel detail.
