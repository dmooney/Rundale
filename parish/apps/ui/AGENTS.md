# parish/apps/ui — agent scope

Svelte 5 + TypeScript SPA. Single frontend across all three modes (Tauri, web, headless preview). See root [`AGENTS.md`](../../../AGENTS.md) and [`docs/agent/code-style.md`](../../../docs/agent/code-style.md).

## Scoped commands

```sh
just ui-test                           # vitest units
just ui-e2e                            # Playwright (auto-starts server)
just screenshots                       # regenerate docs/screenshots/*.png
npm --prefix parish/apps/ui run dev    # local dev server
npm --prefix parish/apps/ui run check  # svelte-check + tsc
```

## Local gotchas

- **`src/lib/types.ts` must match Rust serde output exactly.** snake_case field names. Drift is silent — frontend gets `undefined` for renamed fields.
- **Svelte 5 runes everywhere** (`$state`, `$derived`, `$effect`, `$props`). No legacy `let:reactive` blocks.
- **Playwright snapshot baselines are committed.** UI changes require regenerating baselines (`just ui-e2e -- -u`) and including diffs in the PR per rule #10.
- **The complete Playwright suite is the shipped-surface contract.** A pull request that replaces the default surface must migrate or explicitly retire every prior E2E assertion in the same change, then pass `just ui-e2e`; a focused smoke spec is not sufficient.
- **Tauri IPC vs HTTP**: same store layer dispatches both. Don't fork transports — use the single `invoke`/`fetch` adapter in `src/lib/ipc.ts`.
- **`__mocks__/` is for vitest only**, not Playwright. E2E hits a real server.
- **`license-clarifications.json` must stay current** — `just notices` rebuilds third-party notices when deps change (rule #7).

## Layout

`src/lib/` shared, `src/routes/` SvelteKit pages, `e2e/` Playwright, `static/` assets, `__mocks__/` vitest stubs.
