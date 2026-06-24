# Visual Raster NPC Selection M18 Plan

1. Add a transparent PNG `npc-select.png` under `parish/apps/visual/assets/cues/`.
2. Update `PixiSceneRenderer` to preload/cache the NPC cue texture alongside hotspot cue textures.
3. Replace `drawNpcHighlights()`'s code-drawn `PIXI.Graphics` ellipse with a `PIXI.Sprite` positioned under each active or selected NPC.
4. Preserve existing NPC labels, hit-testing, `promptForTarget()`, `activateNpc()`, and command fallback behavior.
5. Extend `main-regression.test.mjs` to assert the PNG dimensions and reject the old NPC highlight geometry path.
6. Add a browser proof script for M18 that loads a live visual client, hovers/selects NPCs in Kilteevan Village and Darcy's Pub, captures desktop/mobile screenshots, and records prompt/command-input state.
7. Verify with:
   - `npm --prefix parish/apps/visual run test`
   - `npm --prefix parish/apps/visual run check`
   - `npm --prefix parish/apps/visual run build`
   - `npm --prefix parish/apps/visual run audit:atoms`
   - `cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/fixtures/play_visual-raster-npc-selection-m18.txt`
   - live browser proof and `bash parish/scripts/agent-check.sh --source=local`
