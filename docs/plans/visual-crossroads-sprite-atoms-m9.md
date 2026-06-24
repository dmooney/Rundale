# Plan: visual-crossroads-sprite-atoms-m9

1. Audit current Crossroads atoms and identify which are full-frame versus
   object-local.
2. Generate or derive local transparent PNG atoms for the major Crossroads
   objects while preserving the current art direction.
3. Update `mods/rundale/scenes.json` so object layers use local positions,
   anchors, scales, and z-order instead of full-frame `50,50` placement.
4. Add regression tests that fail if Crossroads object layers all regress to
   full-frame centered slices.
5. Run the M9 fixture plus visual client checks/build.
6. Capture desktop/mobile Crossroads screenshots and judge whether the sprite
   composition still carries the loved look.
