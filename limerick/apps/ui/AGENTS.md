# limerick/apps/ui — agent scope

Svelte 5 + TypeScript SPA. Single frontend across all three modes (Tauri, web, headless preview). See root [`AGENTS.md`](../../../AGENTS.md) and [`docs/agent/code-style.md`](../../../docs/agent/code-style.md).

## Scoped commands

```sh
just ui-test                           # vitest units
just ui-e2e                            # Playwright (auto-starts server)
just screenshots                       # regenerate docs/screenshots/*.png
npm --prefix limerick/apps/ui run dev    # local dev server
npm --prefix limerick/apps/ui run check  # svelte-check + tsc
```

## Local gotchas

- **`src/lib/types.ts` must match Rust serde output exactly.** snake_case field names. Drift is silent — frontend gets `undefined` for renamed fields.
- **Svelte 5 runes in components** (`$state`, `$derived`, `$effect`, `$props`). No legacy `let:` reactive blocks. Stores keep the `svelte/store` contract; see [`src/AGENTS.md`](src/AGENTS.md).
- **Playwright snapshot baselines are committed.** A visible UI change regenerates them (`npx playwright test --update-snapshots` in `limerick/apps/ui`, after `just ui-build` in `limerick/`; `just ui-e2e` takes no arguments) and the PR includes the baseline diffs.
- **Playwright has two explicit projects.** `ui-contract` installs the Tauri IPC mock and checks deterministic component/event contracts; `browser-fullstack` must not install it and drives a real `limerick-server` session through the browser HTTP/WS transport. The complete suite is the shipped-surface contract.
- **Tauri IPC vs HTTP**: the same store layer dispatches both. Don't fork transports; `src/lib/ipc.ts` is the single adapter.
- **Most Playwright specs intentionally mock Tauri IPC.** Keep them in the `ui-contract` project and never describe them as end-to-end engine coverage. `fullstack.spec.ts` owns the real browser/server acceptance path.
- **`license-clarifications.json` must stay current** — `just notices` rebuilds third-party notices when deps change ([README maintenance](../../../docs/agent/engineering-rules.md#readme-maintenance)).

## Layout

`src/lib/` shared, `src/routes/` SvelteKit pages, `e2e/` Playwright, `static/` assets, `__mocks__/` vitest stubs.
