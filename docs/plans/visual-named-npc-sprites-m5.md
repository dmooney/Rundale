# Plan: Visual Named NPC Sprites M5

1. Generate a raster pixel-art sprite-sheet source for Padraig Darcy, Niamh
   Darcy, and Peig Hannigan using the built-in image generation path, with a
   removable chroma-key background.

2. Post-process the generated source:
   - remove chroma key,
   - crop the three characters,
   - downsample/pixel-fit them to the client sprite footprint,
   - save project-local PNGs under `mods/rundale/assets/scenes/sprites/`.

3. Update `mods/rundale/scenes.json`:
   - add `sprites` entries for NPC ids `1`, `8`, and `22`,
   - keep `fallback_sprites.default` unchanged.

4. Add tests:
   - real Rundale scene-index test for named sprite definitions,
   - route/core tests that named sprite URLs win over fallback where visible.

5. Verify:
   - run visual `check`, `test`, and `build`,
   - run targeted Rust tests,
   - run the M5 script fixture,
   - run live browser proof with desktop/mobile screenshots and NPC selection.

6. Complete `.proofs/visual-named-npc-sprites-m5/` with evidence, judge, and
   `agent-check`.
