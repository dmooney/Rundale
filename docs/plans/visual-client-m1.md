# Visual Client M1 Plan

## Commit 1: `feat: add visual client shell`

- Add `parish/apps/visual` as a standalone browser app.
- Keep dependencies minimal: use Node scripts and browser Canvas 2D only.
- Implement a configurable backend URL via a small settings form and
  `localStorage`.
- Fetch `/api/scene-state` and render loading, error, null, and scene-present
  states.
- Post movement commands to `/api/command` so the visual app can move its own
  browser session before refreshing scene-state.
- Draw a non-polished scene placeholder on canvas using backend-derived scene
  data: plate URL label, hotspot boxes, slot markers, NPC labels, and overflow
  list.
- Add unit tests for the scene-state client and renderer view model.

## Commit 2: `docs: document visual client milestone`

- Add app-level README and script documentation.
- Add `just` recipes if they are useful without making existing UI recipes more
  complex.
- Capture a browser screenshot for the proof bundle.

## Verification

- `npm --prefix parish/apps/visual run check`
- `npm --prefix parish/apps/visual run test`
- `npm --prefix parish/apps/visual run build`
- Browser proof against a running Parish server on `http://127.0.0.1:3030`.
- `cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/fixtures/play_visual-client-m1.txt`
- `just agent-check`
