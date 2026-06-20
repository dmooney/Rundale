# Visual Client M6 Plan

## Commit: `fix: stabilize visual client layout`

- Update `parish/apps/visual/src/styles.css` so the desktop shell is bound to
  viewport height and the inspector scrolls independently.
- Keep the existing mobile breakpoint as a stacked document by resetting the
  desktop height and overflow constraints.
- Update the visual client README with the desktop/stacked layout behavior.
- Add a deterministic backend fixture that proves the scene sequence used by
  the browser proof.

## Verification

- `npm --prefix parish/apps/visual run check`
- `npm --prefix parish/apps/visual run test`
- `npm --prefix parish/apps/visual run build`
- Browser proof against `http://127.0.0.1:4174/` proxying to a local Parish
  backend on `http://127.0.0.1:3030`, including desktop and mobile viewport
  screenshots.
- `cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/fixtures/play_visual-client-m6.txt`
- `just agent-check`
