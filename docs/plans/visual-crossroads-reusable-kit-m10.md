# Plan: visual-crossroads-reusable-kit-m10

1. Create small transparent Crossroads kit PNG atoms from the current raster
   art, focused on road wetness/puddles.
2. Add reusable kit assets to `mods/rundale/scenes.json`.
3. Add several Crossroads layers that reference those kit assets at different
   coordinates, scales, opacities, and z-orders.
4. Extend visual regression tests so the scene must contain repeated kit asset
   references and small PNG dimensions.
5. Run the M10 headless fixture, visual tests/check/build, live browser proof,
   and local agent-check.
6. Judge the screenshots for repeated-stamp artifacts and document remaining
   debt.
