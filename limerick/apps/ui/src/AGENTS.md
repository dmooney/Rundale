# limerick/apps/ui/src — agent scope

Svelte 5 + TypeScript application source. Commands and frontend-wide traps
(serde type parity, runes, Playwright projects) are in the parent
[`../AGENTS.md`](../AGENTS.md); style is in
[`docs/agent/code-style.md`](../../../../docs/agent/code-style.md).

## Local gotchas

- **`lib/ipc.ts` is the only transport.** Components never import Tauri
  `invoke` or HTTP `fetch` adapters directly; the IPC modules under `lib/ipc/`
  and `lib/editor-ipc.ts` sit behind it.
- **Stores keep the `svelte/store` contract.** Components use runes; stores in
  `stores/` still use `writable`/`derived`, bridged by `$store`
  auto-subscription. The runes migration of the store layer is a tracked
  follow-up to #1366 §4; do not mix paradigms within one store file.
- **Tests are co-located.** A component or `lib/` utility with logic has a
  `*.test.ts` beside it. Vitest only; `__mocks__/` holds vitest stubs, not
  Playwright fixtures.
- **Designer (editor) code is separate from the play surface.** Its
  components are under `components/editor/`, its route under `routes/editor/`,
  and its IPC and types in `lib/editor-*.ts` and `lib/editor/`.
- **Resolve MapLibre glyph roots before appending template tokens.** `new URL('/map-glyphs/{fontstack}/{range}.pbf', document.baseURI)` percent-encodes the braces, so resolve `/map-glyphs/` first and append the tokens afterward. Bundled PBFs also keep label metrics stable across web and Tauri; MapLibre's local-font fallback is platform-dependent.
- **MapLibre 6's worker must be bundled, not merely copied.** Its ESM worker imports `maplibre-gl-shared.mjs`; Vite `?url` emits only the entry file and leaves that sibling missing at runtime. Import `maplibre-gl-worker.mjs?worker&url`, pass the emitted URL to `setWorkerUrl` before constructing any map, and keep the production glyph Playwright contract as the live oracle.
- **A stream flush must consume the triggering action key.** `flushStream` can synchronously clear `$streamingActive`; returning control to generic key dispatch then turns the same ArrowUp/ArrowDown into command-history navigation. Treat the first non-modifier after streaming as a flush-only event and test it alongside history.
- **Sticky chat follows rendered revisions, not array length.** Stream chunks replace one `textLog` entry while its bubble grows; keep bottom-follow tied to every transcript revision and ResizeObserver layout changes, with browser geometry assertions that the reply remains above the composer (#1835).
- **Authoritative streamed-dialogue completion belongs in the terminal event.** Paced `stream-token` buffers can still contain only a first-word prefix when finalization arrives; commit full validated text (or discard partials and render recovery) from `stream-turn-end` (#1855, #1857).
