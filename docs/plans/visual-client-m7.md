# Visual Client M7 Plan

## Commit: `feat: add visual client status`

- Add `parish/apps/visual/src/client-status.js` with pure status label and
  disabled-state helpers.
- Add unit tests for status labels and control disabled-state behavior.
- Add a status line to the visual client inspector.
- Wire refresh, command submission, and error handling to set status state.
- Disable connect, refresh, command, shortcut, and quick-action buttons while
  refresh/command work is in flight.
- Update styles and README copy for the status layer.
- Add a deterministic backend fixture that proves the scene sequence used by
  browser proof.

## Verification

- `npm --prefix parish/apps/visual run check`
- `npm --prefix parish/apps/visual run test`
- `npm --prefix parish/apps/visual run build`
- Browser proof against `http://127.0.0.1:4174/` proxying to a local Parish
  backend on `http://127.0.0.1:3030`, including ready, empty, and error states.
- `cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/fixtures/play_visual-client-m7.txt`
- `just agent-check`
