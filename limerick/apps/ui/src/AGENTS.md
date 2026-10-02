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
