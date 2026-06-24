# Visual Clickable World Proof M22 Plan

1. Add M22 acceptance criteria, deterministic fixture, design note, and implementation plan.
2. Add invisible interaction telemetry to `parish/apps/visual/src/main.js` for hover, select, activate, inspect, movement command submission, transition start, and UI prompt state.
3. Add regression tests that assert the telemetry is browser-only, no visible debug/proof text is added, and the interaction path still derives commands from activation hints rather than display-label parsing.
4. Add a live browser proof that drives the Pixi canvas with pointer events:
   - hover Kilteevan's road-to-crossroads hotspot and capture cue telemetry/screenshot,
   - click it to move to The Crossroads,
   - click The Crossroads pub-lane hotspot to move to Darcy's Pub,
   - click Darcy's Pub hearth inspect hotspot,
   - click the behind-bar NPC sprite.
5. Verify with:
   - `npm --prefix parish/apps/visual run test`
   - `npm --prefix parish/apps/visual run check`
   - `npm --prefix parish/apps/visual run build`
   - `npm --prefix parish/apps/visual run audit:atoms`
   - `cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/fixtures/play_visual-clickable-world-proof-m22.txt`
   - live browser click proof
   - `bash parish/scripts/agent-check.sh --source=local`
