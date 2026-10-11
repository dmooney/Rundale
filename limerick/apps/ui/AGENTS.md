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
- **Person-art scripts have their own traps.** Before changing `scripts/*notebook-person*` or anything under `art/`, read [`art/notebook-person-art/AGENTS.md`](art/notebook-person-art/AGENTS.md).
- **`license-clarifications.json` must stay current** — `just notices` rebuilds third-party notices when deps change ([README maintenance](../../../docs/agent/engineering-rules.md#readme-maintenance)).
- **`limerick/apps/ui/` has its OWN Prettier config, and `just ui-format-check` runs `prettier --check .` from inside `apps/ui/` — so any `apps/ui/**/\*.md`(e.g.`apps/ui/TODO.md`) must be formatted with the UI-local Prettier, not just the root one.** The root `npm run format:check`glob skips`limerick/apps/ui/\*\*`and root markdownlint excludes it too, so a UI-dir markdown file can pass every root doc gate yet still fail`just check`at the`ui-format-check`recipe (table-column padding differs between the two configs). After editing`apps/ui/TODO.md`, run `eval "$(fnm env)" && cd limerick/apps/ui && npx prettier --write TODO.md`; both configs then agree on the result. Also note `ui-check`/`ui-lint`/`ui-format-check`need`limerick/apps/ui/node_modules` (`npm ci`there) — a fresh worktree fails them with`svelte-kit: command not found` (exit 127) until installed.
- **Backend `TimeOfDay` buckets (limerick-types/src/time.rs) are Dawn 5–7, Morning 7–10, Midday 10–14, Afternoon 14–17, Dusk 17–19, Night 19–23, Midnight 23–5.** The StatusBar's client-side interpolated label must mirror these exactly (it drifted once: "Late Morning"/"Evening" buckets that the backend never emits). Web-mode `worldState` only updates on world-update pushes (movement etc.) — the visible clock between pushes is client-interpolated, so snapshot-driven UI (e.g. chat time-rules) lags until the next push.
- **The status-bar bug/Designer/Dbg/Mod buttons live behind the ⋯ dev menu** (`[aria-label="Developer tools menu"]`) since the game-ui-design pass; e2e specs must open the menu before clicking them, and the sidebar now has two `<details>` sections (Present + language hints) — use `.first()`/`:has-text()` instead of a bare `details` locator.
- **Playwright's managed server must never reuse a listener on its test port.** The config allocates a free loopback port by default and uses a per-run readiness identity; set `LIMERICK_TEST_PORT=<unused>` only when a proof needs a stable explicit port (#1717).
- **The Playwright managed-server command must build the UI before snapshotting `dist`.** CI and `just ui-e2e` may prebuild and hide this dependency, while direct/package Playwright, baseline updates, and screenshot commands otherwise serve missing or stale UI. Keep the build inside the launcher lifecycle; invoke the validated absolute `npm_execpath` through `process.execPath` with `shell: false`, including on Windows (#1717).
- **`limerick-server` binaries are coupled to the UI `dist` tree present at compile time.** Playwright's helper snapshots the UI, embeds a worktree/snapshot identity, and validates a content-addressed copy plus live readiness; a shared Cargo output path or CSP-only check is not proof of ownership (#1717).
- **Playwright shutdown is platform-specific and must be tested through the real process manager.** On POSIX, Playwright 1.61 signals the detached process group, so the Node launcher must catch `SIGTERM`/`SIGINT`, stay alive, stop/wait its Rust child, and only then release the lease. On Windows it uses `taskkill /T /F`, which can skip Node cleanup entirely; bounded lease retirement is the fallback, not a claimed normal release (#1717).
- **Playwright's cached UI snapshot stays live for the whole server run.** Axum's `ServeDir` rereads it per request, so a lease heartbeat failure must fence the child before pruning can reclaim its binary+snapshot. Stale leases first become tombstones for a full grace and only a later pass reclaims them. Pruning retains up to three entries per worktree cache directory and artifact type; 24-hour age bounds cross-worktree residue (#1717).
- **Playwright may evaluate `playwright.config.ts` in more than one process.** A dynamically allocated port must be published through `LIMERICK_TEST_PORT` before workers start, or the managed server and browser `baseURL` can silently receive different ports (#1717).
- **Browser pixel proofs need a real page origin.** Playwright request contexts can fetch root-relative assets while an `about:blank` page cannot; navigate to the served artifact before using in-page `fetch()` or `createImageBitmap()`.
- **Shell-free npm automation must run npm's JavaScript entry point.** Use `process.execPath` plus a validated `npm_execpath` (or explicit JS-path override), not `npm.cmd`; for generated-file replacement, use unique same-directory candidates and make the final manifest re-read immediately precede rename. A hard-killed run may leave hidden nonblocking residue, but cannot corrupt the destination or block later runs.
- **Canonical notebook screenshots must use approved Rundale NPC identities.** The default E2E NPCs are invented and exercise fallback art; `screenshots.spec.ts` owns Playwright baselines under `limerick/apps/ui/e2e/screenshots/baseline/`, while the README hero is the separately curated `docs/screenshots/rundale.png` artifact.
- **Playwright pixel baselines must include the rendering platform in their identity.** Full-page screenshots containing text, emoji, or map tiles can differ materially between macOS and Linux even when behavior and CSS are identical; use `{platform}` in `snapshotPathTemplate` instead of weakening the visual-diff threshold.
- **A Playwright readiness deadline must preserve the last completed probe's
  semantic error.** The final fetch can straddle the shared deadline and abort;
  do not replace an ownership or identity diagnosis with that timer artifact.

## Layout

`src/lib/` shared, `src/routes/` SvelteKit pages, `e2e/` Playwright, `static/` assets, `__mocks__/` vitest stubs.
