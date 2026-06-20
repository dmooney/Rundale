# Visual Client M4 Plan

## Commit: `feat: add visual play transcript`

- Add `parish/apps/visual/src/turn-log.js` with pure transcript helpers for
  response summarization and bounded append behavior.
- Add unit tests for response extraction, local event entries, and max-entry
  trimming.
- Extend `parish/apps/visual/index.html` and `src/styles.css` with a compact
  recent-turn transcript.
- Wire `parish/apps/visual/src/main.js` so command submissions, backend
  responses, inspect hotspot clicks, and NPC sprite selections append entries.
- Update the visual client README with the transcript behavior.
- Add a deterministic backend fixture that proves the scene sequence used by
  the browser proof.

## Verification

- `npm --prefix parish/apps/visual run check`
- `npm --prefix parish/apps/visual run test`
- `npm --prefix parish/apps/visual run build`
- Browser proof against `http://127.0.0.1:4174/` proxying to a local Parish
  backend on `http://127.0.0.1:3030`.
- `cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/fixtures/play_visual-client-m4.txt`
- `just agent-check`
