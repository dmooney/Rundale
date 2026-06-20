# Visual Client M3 Plan

## Commit: `feat: render visual npc sprites`

- Extend `parish/apps/visual/src/renderer.js` with sprite baseline dimensions,
  NPC sprite bounds, NPC hit-testing, selected/hovered NPC highlighting, and
  image drawing with a fallback marker.
- Extend `parish/apps/visual/src/main.js` to load NPC sprite URLs, track active
  hotspots and NPCs separately, prefer NPC clicks over hotspot clicks, and fill
  the command input with `talk to <display label>` on sprite activation.
- Add unit tests for sprite bounds, NPC hit-testing, click-action derivation,
  and NPC-over-hotspot precedence.
- Update the visual client README with sprite-click behavior.
- Add a deterministic backend fixture that proves the pub scene includes
  sprite-backed NPCs.

## Verification

- `npm --prefix parish/apps/visual run check`
- `npm --prefix parish/apps/visual run test`
- `npm --prefix parish/apps/visual run build`
- Browser proof against `http://127.0.0.1:4174/` proxying to a local Parish
  backend on `http://127.0.0.1:3030`.
- `cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/fixtures/play_visual-client-m3.txt`
- `just agent-check`
