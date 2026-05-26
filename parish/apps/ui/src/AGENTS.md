# parish/apps/ui/src -- agent scope

Svelte 5 + TypeScript application source. The actual frontend code backing the project shell described in [`../AGENTS.md`](../AGENTS.md). See root [`AGENTS.md`](../../../../AGENTS.md) and [`docs/agent/code-style.md`](../../../../docs/agent/code-style.md).

## Scoped commands

```sh
pnpm --dir parish/apps/ui run check    # svelte-check + tsc (from project root)
pnpm --dir parish/apps/ui run test     # vitest (from project root)
```

Commands are run from the workspace root. Use `pnpm --dir parish/apps/ui` -- do not `cd` into this subtree.

## Local gotchas

- **`src/lib/types.ts` must match Rust serde output exactly.** snake_case field names. Drift is silent -- frontend gets `undefined` for renamed fields. This is the single most critical seam in the frontend.
- **Svelte 5 runes everywhere** (`$state`, `$derived`, `$effect`, `$props`). No legacy `let:` reactive blocks. Stores in `src/stores/` also use runes-based state (`game.ts` is the primary game state store).
- **IPC bridge (`src/lib/ipc.ts`) dispatches transparently to both Tauri `invoke` and HTTP `fetch`.** Do not fork transports or import Tauri/HTTP adapters directly in components -- always go through this single adapter.
- **Tests are co-located.** Every `.svelte` component with logic should have a `*.test.ts` beside it. Vitest only -- `__mocks__/` is for vitest stubs, not Playwright.
- **Editor components live under `components/editor/`**, the Designer page route under `routes/editor/`. Editor IPC and types are in `lib/editor-ipc.ts`, `lib/editor-map.ts`, `lib/editor-types.ts`.
- **Map rendering utilities** live under `lib/map/` (controller, GeoJSON, style, tile sync).
- **Save-picker logic** lives under `lib/save-picker/` (DAG tree, ledger list).
- **Setup/onboarding logic** lives under `lib/setup/` (download rate, setup messages, storage, stream manager).
- **Build-time assets** go in `lib/assets/` (only `favicon.svg` currently).
- **`app.css`** is global styles; `app.d.ts` is TypeScript declarations; `app.html` is the SvelteKit HTML shell; `test-setup.ts` is vitest test setup.

## Module map

`components/` Svelte components, `lib/` shared utilities + types + IPC, `routes/` SvelteKit pages, `stores/` runes-based state stores, `__mocks__/` vitest stubs.

### Components (43 entries)

`ChatPanel.svelte`, `InputField.svelte`, `Sidebar.svelte`, `MapPanel.svelte`, `StatusBar.svelte` -- the primary game HUD. `DebugPanel.svelte` with `<Debug*Tab>` sub-components (Overview, Npcs, Events, Conversations, Gossip, Inference, Weather, World). `FullMapOverlay.svelte`, `MapTooltip.svelte` -- map interactions. `SetupOverlay.svelte`, `SavePicker.svelte`, `ByokOnboarding.svelte`, `ByokFork.svelte`, `LocalInferenceFork.svelte`, `ModSelectorOverlay.svelte` -- onboarding and configuration. `MentionDropdown.svelte`, `SlashDropdown.svelte`, `ModelDropdown.svelte` -- input helpers. `AuthStatus.svelte`, `MoodIcon.svelte` -- status indicators. `DemoBanner.svelte`, `DemoPanel.svelte` -- demo mode UI. `components/editor/` contains the Parish Designer component tree (8 entries: `LocationDetail`, `LocationList`, `ModBrowser`, `NpcDetail`, `NpcList`, `SaveInspector`, `ValidatorPanel`).

### Shared library (37 entries)

`lib/types.ts` -- Rust-backed IPC types (the seam). `lib/ipc.ts` -- single adapter for Tauri `invoke` and HTTP `fetch`. `lib/map/` -- map rendering (controller, GeoJSON, style, tileSync). `lib/save-picker/` -- save DAG and ledger UI. `lib/setup/` -- onboarding orchestration (stream manager, download rate, setup messages, storage). `lib/assets/` -- build-embedded static assets. Individual utility modules: `app-icon`, `auto-pause`, `byokProviders`, `demo-player`, `editor-ipc`, `editor-map`, `editor-types`, `map-icons`, `model-catalog`, `reactions`, `rich-text`, `scene-dedup`, `screenshot`, `setupWaitMessages`, `slash-commands`, `stream-pacing`, `theme`. Each utility with logic has a co-located `.test.ts`.

### Routes

`+layout.svelte` + `+layout.ts` -- root layout and loader. `+page.svelte` -- main game page. `routes/editor/` (with `+page.svelte` + `+page.ts`) -- the Parish Designer page.

### Stores (12 entries)

`game.ts` -- primary game state store (`+game.test.ts`). `debug.ts`, `demo.ts`, `editor.ts`, `nouns.ts`, `save.ts`, `theme.ts`, `tiles.ts` (`+tiles.test.ts`), `travel.ts` (`+travel.test.ts`). All use Svelte 5 runes.
