# Visual Canvas Fallback Game Read M19 Plan

1. Add acceptance criteria, a deterministic `/scene` fixture, and this scoped design/plan bundle.
2. Update `parish/apps/visual/src/renderer.js` so `renderSceneModel()` no longer draws the scene title, slot overlays, all hotspot rectangles, all hotspot labels, all NPC labels, or NPC mood emoji on first read.
3. Add canvas fallback cue helpers that draw only active or selected hotspot/NPC cues, using quiet ellipse/arc marks instead of full debug boxes.
4. Preserve `buildSceneDisplayModel()`, hit-testing helpers, hotspot command resolution, NPC command resolution, and sprite drawing behavior.
5. Extend `parish/apps/visual/src/main-regression.test.mjs` to reject the old fallback debug overlay snippets and assert the new active-only cue helpers exist.
6. Add a proof script under `.proofs/visual-canvas-fallback-game-read-m19/` that renders a sample scene model through a mock canvas context and records the absence of inactive debug draw calls plus the presence of active cues.
7. Verify with:
   - `npm --prefix parish/apps/visual run test`
   - `npm --prefix parish/apps/visual run check`
   - `npm --prefix parish/apps/visual run build`
   - `npm --prefix parish/apps/visual run audit:atoms`
   - `cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/fixtures/play_visual-canvas-fallback-game-read-m19.txt`
   - fallback draw-call proof
   - live visual-client smoke proof
   - `bash parish/scripts/agent-check.sh --source=local`
