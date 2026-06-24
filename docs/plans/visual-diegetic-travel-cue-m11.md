# Plan: visual-diegetic-travel-cue-m11

1. Update the Pixi travel hotspot renderer to draw path glints instead of
   downward chevrons.
2. Adjust regression tests so they reject the old chevron helper and require
   the new travel glint helper.
3. Add a deterministic M11 script fixture.
4. Run visual tests/check/build plus the M11 script fixture.
5. Capture live desktop/mobile screenshots and judge whether the cue reads
   without looking like a debug overlay.
