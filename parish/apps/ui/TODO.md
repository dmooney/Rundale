# parish/apps/ui — Technical Debt

## Open

_(none — see Follow-up for items deferred due to risk/scope)_

## In Progress

_(none)_

## Done

| ID     | Category            | Severity | Status | Description                                                                                                                      |
| ------ | ------------------- | -------- | ------ | -------------------------------------------------------------------------------------------------------------------------------- |
| TD-001 | Duplication         | P1       | Fixed  | MapLibre mock extracted to `__mocks__/maplibre-gl.ts`; both test files use `vi.mock('maplibre-gl')` without factory.             |
| TD-004 | Duplication         | P3       | Fixed  | `typeIntoEditor()` extracted to module-level function in `InputField.test.ts`; 7 local copies removed.                           |
| TD-005 | Weak Tests          | P1       | Fixed  | Added tests for `stopDemo()` (direct setter) and `runDemoTurn()` early-return paths (disabled/paused).                           |
| TD-006 | Weak Tests          | P1       | Fixed  | Added `travel.test.ts` covering `startTravel()`, `cancelTravel()`, clamping, mutual-cancellation (#349), and auto-clear.         |
| TD-007 | Weak Tests          | P2       | Fixed  | Added `reactions.test.ts` — validates 12 entries, unique emoji/keys, non-empty descriptions.                                     |
| TD-008 | Weak Tests          | P2       | Fixed  | Added `map-icons.test.ts` — validates ICON_PATHS keys and non-empty paths; coverage for all 14 NAME_RULES patterns.              |
| TD-009 | Weak Tests          | P2       | Fixed  | Added `theme.test.ts` — round-trip, corrupt JSON returns default, missing key returns default, quota-exceeded graceful handling. |
| TD-014 | Weak Tests          | P3       | Fixed  | Added `AuthStatus.test.ts` — covers onMount fetch, logged-in state, login link, Tauri-bypass branch.                             |
| TD-015 | Weak Tests          | P3       | Fixed  | Added `DemoBanner.test.ts` — covers visibility, turn count, pause/resume toggle, status label, Stop button.                      |
| TD-016 | Weak Tests          | P3       | Fixed  | Added `DemoPanel.test.ts` — covers field rendering, Apply & Start, Pause/Resume, Stop, turn count, status.                       |
| TD-023 | Dead Code           | P2       | Fixed  | Removed `export const prerender = true` from `+layout.ts` (no-op with `ssr=false`).                                              |
| TD-024 | Dead Code           | P3       | Fixed  | Deleted `src/lib/index.ts` (empty placeholder barrel file).                                                                      |
| TD-025 | Stale Docs/Comments | P2       | Fixed  | Updated TODO comment in `style.ts` to factual doc — removed `TODO:` prefix, turned into `Known limitation`.                      |
| TD-026 | Stale Docs/Comments | P3       | Fixed  | Added `/unexplored` to `FEATURES_MD_COMMANDS` and removed from `REGISTRY_ONLY_COMMANDS` — resolves self-contradiction.           |
| TD-027 | Config/Deps         | P2       | Fixed  | Added documented note to `package.json` explaining the `cookie` override with GHSA references.                                   |
| TD-028 | Config/Deps         | P2       | Fixed  | Removed manual `declare const process` from `vite.config.ts` — `@types/node` is already a devDependency.                         |
| TD-029 | Config/Deps         | P3       | Fixed  | Removed `rewriteRelativeImportExtensions: true` from `tsconfig.json` — no `.ts`-extension imports exist.                         |
| TD-030 | Weak Tests          | P2       | Fixed  | Added tests for `addReaction()` (player replacement, NPC append) and `removeReaction()` in `game.test.ts`.                       |

## Follow-up

Items deferred due to risk, scope, or requiring changes outside this crate:

| ID     | Category    | Severity | Description                                                                                                    |
| ------ | ----------- | -------- | -------------------------------------------------------------------------------------------------------------- |
| TD-002 | Duplication | P2       | Extract `<MapTooltip>` component from shared HTML in MapPanel + FullMapOverlay. Requires new Svelte component. |
| TD-003 | Duplication | P2       | Extract shared `$effect` tile-source block from MapPanel + FullMapOverlay.                                     |
| TD-010 | Weak Tests  | P2       | Add Weather/Gossip/Conversation tab rendering tests to DebugPanel.                                             |
| TD-011 | Weak Tests  | P2       | Add IPC failure-path tests to SavePicker (loadBranch, createBranch, newSaveFile, newGame).                     |
| TD-012 | Weak Tests  | P2       | Add reaction IPC failure rollback, tabular subtype, scroll-to-bottom tests to ChatPanel.                       |
| TD-013 | Weak Tests  | P2       | Add error-state rendering test (`hasError` branch) to SetupOverlay.                                            |
| TD-017 | Weak Tests  | P2       | Add Playwright E2E coverage for debug panel, save picker, settings, editor, reactions, sidebar toggle.         |
| TD-018 | Complexity  | P1       | Split 406-line `setupMount()` in `+page.svelte` into composable modules.                                       |
| TD-019 | Complexity  | P1       | Extract mention/slash/model dropdowns from 1321-line `InputField.svelte`.                                      |
| TD-020 | Complexity  | P2       | Extract per-tab components from 1083-line `DebugPanel.svelte`.                                                 |
| TD-021 | Complexity  | P2       | Extract ledger list view and DAG tree view from 786-line `SavePicker.svelte`.                                  |
| TD-022 | Complexity  | P2       | Split download-rate tracking and message formatting from 1059-line `SetupOverlay.svelte`.                      |
