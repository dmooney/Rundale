# Visual Atom-Only Compositor Proof M20 Plan

1. Add M20 acceptance criteria, deterministic `/scene` fixture, design note, and implementation plan.
2. Update `PixiSceneRenderer` to accept a proof/atom-only option that suppresses legacy underlay/plate fallback drawing and records compositor telemetry after each scene render.
3. Update `main.js` to derive proof atom mode from a URL query parameter and pass it into `PixiSceneRenderer`, with no visible UI changes.
4. Add visual regression tests that assert telemetry is present, plate fallback is tracked, proof mode is query-string-only, and no debug overlay text is added to `index.html`.
5. Add a live browser proof script that loads the visual client with atom-only proof mode, visits Kilteevan Village, The Crossroads, and Darcy's Pub, captures screenshots, and records telemetry/metrics.
6. Verify with:
   - `npm --prefix parish/apps/visual run test`
   - `npm --prefix parish/apps/visual run check`
   - `npm --prefix parish/apps/visual run build`
   - `npm --prefix parish/apps/visual run audit:atoms`
   - `cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/fixtures/play_visual-atom-only-compositor-proof-m20.txt`
   - live browser proof
   - `bash parish/scripts/agent-check.sh --source=local`
