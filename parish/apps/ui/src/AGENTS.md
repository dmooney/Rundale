# parish/apps/ui/src — agent scope

Svelte 5 + TypeScript application source. See parent [`../AGENTS.md`](../AGENTS.md) for frontend-level rules and root [`AGENTS.md`](../../../../AGENTS.md) and [`docs/agent/code-style.md`](../../../../docs/agent/code-style.md).

## Scoped commands

```sh
pnpm --dir parish/apps/ui run check    # svelte-check + tsc (from project root)
pnpm --dir parish/apps/ui run test     # vitest (from project root)
```

Run from the workspace root. Use `pnpm --dir parish/apps/ui` — do not `cd` into this subtree.

## Local gotchas

- **`src/lib/types.ts` must match Rust serde output exactly.** snake_case field names. Drift is silent — frontend gets `undefined` for renamed fields. This is the single most critical seam.
- **Svelte 5 runes everywhere** (`$state`, `$derived`, `$effect`, `$props`). No legacy `let:` reactive blocks. Stores in `src/stores/` also use runes-based state.
- **IPC bridge (`src/lib/ipc.ts`) is the only transport.** Do not import Tauri `invoke` or HTTP `fetch` adapters directly in components.
- **Tests are co-located.** Every `.svelte` component with logic has a `*.test.ts` beside it. Vitest only — `__mocks__/` is for vitest stubs, not Playwright.
- **Editor components** live under `components/editor/`; the Designer page route under `routes/editor/`. Editor IPC and types are in `lib/editor-ipc.ts`, `lib/editor-map.ts`, `lib/editor-types.ts`, `lib/editor/`.
- **Map rendering** lives under `lib/map/` (controller, GeoJSON, style, tile sync).
- **Save-picker logic** lives under `lib/save-picker/`.
- **Setup/onboarding logic** lives under `lib/setup/`.
- **Input field logic** lives under `lib/input-field/`.
- **IPC modules** live under `lib/ipc/` (in addition to the top-level `lib/ipc.ts` adapter).

## Module map

`components/` Svelte components, `lib/` shared utilities + types + IPC, `routes/` SvelteKit pages, `stores/` runes-based state, `__mocks__/` vitest stubs.

### Components

Primary game HUD: `ChatPanel.svelte`, `InputField.svelte`, `Sidebar.svelte`, `MapPanel.svelte`, `StatusBar.svelte`. Debug panel: `DebugPanel.svelte` with `<Debug*Tab>` sub-components (Overview, Npcs, Events, Conversations, Gossip, Inference, Weather, World). Map: `FullMapOverlay.svelte`, `MapTooltip.svelte`. Onboarding/config: `SetupOverlay.svelte`, `SavePicker.svelte`, `ByokOnboarding.svelte`, `ByokFork.svelte`, `LocalInferenceFork.svelte`, `ModSelectorOverlay.svelte`. Input helpers: `MentionDropdown.svelte`, `SlashDropdown.svelte`, `ModelDropdown.svelte`. Status: `AuthStatus.svelte`, `MoodIcon.svelte`. Demo: `DemoBanner.svelte`, `DemoPanel.svelte`. Bug reporting: `BugChip.svelte`, `BugReportModal.svelte`. UI chrome: `ShortcutsOverlay.svelte`. `components/editor/` contains the Parish Designer tree: `LocationDetail`, `LocationList`, `ModBrowser`, `NpcDetail`, `NpcList`, `SaveInspector`, `ValidatorPanel`.

### Shared library (`lib/`)

`types.ts` — Rust-backed IPC types (the seam). `ipc.ts` / `ipc/` — Tauri `invoke` + HTTP `fetch` adapter. `map/` — map rendering. `save-picker/` — save DAG and ledger UI. `setup/` — onboarding orchestration. `input-field/` — input field helpers. `editor/` / `editor-ipc.ts` / `editor-map.ts` / `editor-types.ts` — Designer logic. `assets/` — build-embedded statics. Individual utilities: `app-icon`, `async-loading`, `auto-pause`, `byokProviders`, `demo-player`, `map-icons`, `model-catalog`, `page-controller`, `reactions`, `rich-text`, `scene-dedup`, `screenshot`, `setupWaitMessages`, `slash-commands`, `stream-pacing`, `theme`. Each utility with logic has a co-located `.test.ts`.

### Routes

`+layout.svelte` + `+layout.ts` — root layout. `+page.svelte` — main game page. `routes/editor/` (`+page.svelte` + `+page.ts`) — Parish Designer.

### Stores

`game.ts` — primary game state (`game.test.ts`). `bugReport.ts`, `debug.ts`, `demo.ts`, `editor.ts`, `nouns.ts` (`nouns.test.ts`), `save.ts`, `theme.ts`, `tiles.ts` (`tiles.test.ts`), `travel.ts` (`travel.test.ts`). All use Svelte 5 runes.
