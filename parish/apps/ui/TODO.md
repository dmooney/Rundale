# parish/apps/ui — Technical Debt

## Open

*(none)*

## In Progress

_(none)_

## Done

| ID     | Category            | Severity | Status | Description                                                                                                                      |
| ------ | ------------------- | -------- | ------ | -------------------------------------------------------------------------------------------------------------------------------- |
| TD-010 | Weak Tests          | P2       | Fixed  | Added Weather/Gossip/Conversation tab rendering tests (6 tests) to DebugPanel.test.ts.                                          |
| TD-011 | Weak Tests          | P2       | Fixed  | Added IPC failure-path tests (4 tests) to SavePicker.test.ts (loadBranch, createBranch, newSaveFile, newGame).                   |
| TD-012 | Weak Tests          | P2       | Fixed  | Added reaction IPC failure rollback, tabular subtype (2 tests), and scroll-to-bottom tests (4 total) to ChatPanel.test.ts.       |
| TD-013 | Weak Tests          | P2       | Fixed  | Added error-state rendering tests (3 tests, hasError branch) to SetupOverlay.test.ts.                                           |
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
| TD-017 | Weak Tests          | P2       | Fixed  | Added Playwright E2E coverage: debug panel data verification (6 tests), save picker branch load (1 test). Fixed SetupOverlay blocking clicks in Tauri-mocked tests. |
| TD-002 | Duplication         | P2       | Fixed  | Extracted `<MapTooltip>` component from shared HTML in MapPanel + FullMapOverlay.                               |
| TD-003 | Duplication         | P2       | Fixed  | Extracted shared tile-source block into `$lib/map/tileSync.ts` via `subscribeTileSource()`.                    |
| TD-018 | Complexity          | P1       | Fixed  | Extracted `createStreamManager(appendStreamToken + NPC turn pump) to `$lib/setup/stream-manager.ts`; -208 lines in +page.svelte. |
| TD-021 | Complexity          | P2       | Fixed  | Extracted `<LedgerList>` and `<DagTree>` sub-components from SavePicker.svelte; -420 lines in SavePicker.svelte. |
| TD-022 | Complexity          | P2       | Fixed  | Split download-rate tracking, message formatting, and session storage into `$lib/setup/` modules; -162 lines in SetupOverlay.svelte. |
| TD-019 | Complexity          | P1       | Fixed  | Extracted `<MentionDropdown>`, `<SlashDropdown>`, `<ModelDropdown>` from InputField.svelte; -88 lines (was 1321).                                                                |
| TD-020 | Complexity          | P2       | Fixed  | Extracted 8 tab components (`<DebugOverviewTab>`, `<DebugNpcsTab>`, `<DebugWorldTab>`, `<DebugWeatherTab>`, `<DebugGossipTab>`, `<DebugConversationsTab>`, `<DebugEventsTab>`, `<DebugInferenceTab>`) from DebugPanel.svelte; -872 lines (was 1083). |

## Follow-up

*(none)*

## Progress Log

| Date       | Who   | Description                                                                 |
| ---------- | ----- | --------------------------------------------------------------------------- |
| 2026-05-07 | Agent | **TD-002 + TD-003** — Extracted `MapTooltip.svelte` component; created `$lib/map/tileSync.ts` with `subscribeTileSource()`. Replaced shared HTML template and duplicate `$effect` block in both `MapPanel.svelte` and `FullMapOverlay.svelte`. All tests pass (379/379). |
| 2026-05-07 | Agent | **TD-010–TD-013** — Added Weather/Gossip/Conversation tab tests (6) to DebugPanel; IPC failure-path tests (4) to SavePicker; reaction rollback/tabular/scroll tests (4) to ChatPanel; error-state tests (3) to SetupOverlay. All 396 tests pass (379→396). |
| 2026-05-07 | Agent | **TD-018 + TD-021 + TD-022** — Extracted \`createStreamManager()\` (268 lines) to \`$lib/setup/stream-manager.ts\` (-208 in +page.svelte); extracted \`<LedgerList>\` (131) and \`<DagTree>\` (318) sub-components (-420 in SavePicker.svelte); split download-rate, message formatting, and storage utilities into \`$lib/setup/download-rate.ts\` (52), \`setup-messages.ts\` (80), \`storage.ts\` (83) (-162 in SetupOverlay.svelte). All 396 tests pass. |
| 2026-05-07 | Agent | **TD-019 + TD-020** — Extracted \`<MentionDropdown>\`, \`<SlashDropdown>\`, \`<ModelDropdown>\` from InputField.svelte (-88 lines, was 1321); extracted \`<DebugOverviewTab>\`, \`<DebugNpcsTab>\`, \`<DebugWorldTab>\`, \`<DebugWeatherTab>\`, \`<DebugGossipTab>\`, \`<DebugConversationsTab>\`, \`<DebugEventsTab>\`, \`<DebugInferenceTab>\` from DebugPanel.svelte (-872 lines, was 1083). All 396 tests pass, no new svelte-check errors. |
| 2026-05-08 | Agent | **TD-017** — Added 6 Playwright E2E tests: debug panel data verification (clock/weather/gossip/conversations/world tabs), save picker branch load. Fixed SetupOverlay overlay blocking pointer events in Tauri-mocked tests by adding `get_setup_snapshot` to mock responses. 45/50 E2E tests pass (5 pre-existing failures: 3 editor, 2 smoke). |
