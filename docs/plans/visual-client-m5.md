# Visual Client M5 Plan

## Commit: `feat: add visual quick actions`

- Add `parish/apps/visual/src/action-list.js` with pure helpers for hotspot and
  NPC quick-action labels.
- Add unit tests covering helper labels and fallback text.
- Render Hotspots and People panel entries as full-width buttons instead of
  inert list text.
- Wire sidebar hotspot buttons to the existing inspect/travel activation path.
- Wire sidebar person buttons to the existing NPC selection/talk preparation
  path.
- Update visual client styles and README copy for the quick-action behavior.
- Add a deterministic backend fixture that proves the scene sequence used by
  the browser proof.

## Verification

- `npm --prefix parish/apps/visual run check`
- `npm --prefix parish/apps/visual run test`
- `npm --prefix parish/apps/visual run build`
- Browser proof against `http://127.0.0.1:4174/` proxying to a local Parish
  backend on `http://127.0.0.1:3030`.
- `cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/fixtures/play_visual-client-m5.txt`
- `just agent-check`
