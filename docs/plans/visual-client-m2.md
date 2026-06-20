# Visual Client M2 Plan

## Commit: `feat: render visual scene plates`

- Extend `parish/apps/visual/src/renderer.js` with a renderable scene model,
  plate-image drawing, active hotspot highlighting, and exported hit-testing
  helpers.
- Extend `parish/apps/visual/src/main.js` to load `plate_url`, track hover and
  selection, wire canvas pointer/click events, and submit hotspot commands
  through the existing command bridge.
- Add unit tests for geometry, hit testing, and hotspot command derivation.
- Update the visual client README with canvas interaction behavior.
- Add a deterministic backend fixture that proves the two authored scene plates
  and hotspots are available.

## Verification

- `npm --prefix parish/apps/visual run check`
- `npm --prefix parish/apps/visual run test`
- `npm --prefix parish/apps/visual run build`
- Browser proof against `http://127.0.0.1:4174/` proxying to a local Parish
  backend on `http://127.0.0.1:3030`.
- `cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/fixtures/play_visual-client-m2.txt`
- `just agent-check`
