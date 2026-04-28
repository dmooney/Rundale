# Regression Audit: GUI (Tauri 2 + Svelte 5)

Scope: Chat Panel, Status Bar, Map (minimap + full overlay + tile sources),
Sidebar (NPCs Here, Focail), Theme System (time/season/weather), Save
Picker (F5), Debug Panel (5 tabs), Input Field (@mention, slash autocomplete,
history, quick-travel).

## 1. Sub-features audited

- Chat Panel: log, emote parsing, streaming cursor, auto-scroll, spinner phrases
- Status Bar: location/time/weather/season/festival/debug toggle
- Map: minimap pan/zoom/tween, full overlay (M), tile sources, label collision
- Sidebar: NPCs Here, Focail (Irish words), `/irish` toggle
- Theme System: time-of-day RGB gradient, season/weather tinting, mod accent
- Save Picker (F5): branch DAG, hierarchical layout, auto-zoom bbox
- Debug Panel: Overview/NPCs/World/Events/Inference tabs
- Input Field: contenteditable, @mention chips, slash autocomplete, 50-entry history, quick-travel buttons

## 2. Coverage matrix

Components in `apps/ui/src/components/` and `apps/ui/src/components/editor/` —
**6 of 17 components have a colocated `.test.ts`** (~35%):

| Component | Test file | Status |
|---|---|---|
| `ChatPanel.svelte` | `ChatPanel.test.ts` | covered |
| `InputField.svelte` | `InputField.test.ts` | covered |
| `MapPanel.svelte` | `MapPanel.test.ts` | covered |
| `MoodIcon.svelte` | `MoodIcon.test.ts` | covered |
| `StatusBar.svelte` | `StatusBar.test.ts` | covered |
| `editor/LocationDetail.svelte` | `editor/LocationDetail.test.ts` | covered |
| `AuthStatus.svelte` | — | **none** |
| `DebugPanel.svelte` | — | **none** (5 tabs unverified) |
| `FullMapOverlay.svelte` | — | **none** |
| `SavePicker.svelte` | — | **none** (DAG layout untested) |
| `Sidebar.svelte` | — | **none** (Focail panel untested) |
| `editor/LocationList.svelte` | — | **none** |
| `editor/ModBrowser.svelte` | — | **none** |
| `editor/NpcDetail.svelte` | — | **none** |
| `editor/NpcList.svelte` | — | **none** |
| `editor/SaveInspector.svelte` | — | **none** |
| `editor/ValidatorPanel.svelte` | — | **none** |

Library / store coverage in `apps/ui/src/lib/` + `stores/`:

| File | Coverage |
|---|---|
| `lib/slash-commands.test.ts` | covered |
| `lib/ipc.test.ts` | covered |
| `lib/stream-pacing.test.ts` | covered (token streaming) |
| `lib/rich-text.test.ts` | covered (likely emote parsing) |
| `lib/auto-pause.test.ts` | covered |
| `lib/editor-map.test.ts` | covered |
| `lib/map/geojson.test.ts` | covered |
| `lib/map/controller.test.ts` | covered |
| `lib/map/style.test.ts` | covered |
| `stores/game.test.ts` | covered |
| `stores/tiles.test.ts` | covered |

E2E (`apps/ui/e2e/`):

| Spec | Coverage |
|---|---|
| `smoke.spec.ts` | page loads, can type, can move, API JSON, screenshot at different states (5 tests) |
| `app.spec.ts` | app shell, status bar, chat initial description, map canvas, NPC chip row, input enabled, sidebar pronunciation hints (7 tests) |
| `interactions.spec.ts` | type+Enter, disabled during streaming, multi-npc stream interleaving, paused indicator (5 tests) |
| `screenshots.spec.ts` | screenshot generation + visual regression baselines |

## 3. Strong spots

- The **lib layer** is where this codebase shines for UI testing: slash
  parsing, IPC mocks, stream pacing, rich-text emote rendering, map
  controller/geojson/style, both stores (game, tiles), auto-pause,
  editor-map — 11 test files cover the pure-logic layer.
- E2E suite covers the **golden path**: page-load, typing, movement,
  streaming, paused state, multi-NPC stream interleaving.
- Visual-regression baselines exist (`screenshots.spec.ts`) — catches
  any pixel-level theme or layout drift, which is unusually rigorous.
- 4 of the most-used components (ChatPanel, InputField, MapPanel,
  StatusBar) have direct unit tests.

## 4. Gaps

- **[P0] `SavePicker.svelte` has no test.** This component is reachable
  via F5, renders a branch-DAG with hierarchical layout and auto-zoom
  bbox, and gates the entire save-management UX. Pixel-level visual
  regression is the *only* current sensor and won't catch click-handler
  drift or DAG-layout bugs. Suggested: Vitest test in
  `apps/ui/src/components/SavePicker.test.ts`.
- **[P0] `DebugPanel.svelte` has no test.** features.md describes 5 tabs
  (Overview, NPCs, World, Events, Inference) with non-trivial state
  inspection. A regression on tab switching or snapshot rendering
  ships invisibly. Suggested:
  `apps/ui/src/components/DebugPanel.test.ts` covering each tab's
  basic render path.
- **[P0] Editor pane is largely untested.** 6 of 7 `editor/` components
  (LocationList, ModBrowser, NpcDetail, NpcList, SaveInspector,
  ValidatorPanel) ship with no unit test. The mod editor is shipping
  feature; regressions here corrupt mods. Suggested: minimum smoke
  test per component asserting render + 1 interaction.
- **[P1] Theme System (time-of-day RGB gradient, season/weather tinting)
  has no unit test.** Pixel-regression catches drift in the *current*
  state, not transitions or per-input correctness. Suggested:
  `apps/ui/src/lib/theme.test.ts` asserting the gradient interpolation
  for a few (time, season, weather) tuples.
- **[P1] Spinner color cycling and 25 loading phrases untested.** Mod-driven
  config is exercised at the data layer but no UI test asserts a phrase
  appears or color cycles. Suggested:
  `apps/ui/src/components/Spinner.test.ts` (likely file path) or
  add coverage to `ChatPanel.test.ts`.
- **[P1] `Sidebar.svelte` has no unit test** despite hosting NPCs-Here
  and the Focail (Irish words) panel — both feature-list items.
  E2E asserts pronunciation hints render in `app.spec.ts:7` but the
  toggle behavior of `/irish` is unverified. Suggested:
  `apps/ui/src/components/Sidebar.test.ts`.
- **[P1] `FullMapOverlay.svelte` (M hotkey, full parish overlay) has no
  test.** Distinct component from the minimap; its zoom/pan state and
  tile-source switching (`/map <id>`, gated on `period-map-tiles`
  flag) are unverified. Suggested:
  `apps/ui/src/components/FullMapOverlay.test.ts`.
- **[P2] @mention autocomplete keyboard navigation depth.**
  `InputField.test.ts` exists but it's unclear whether tab/arrow
  navigation through @-mention candidates is asserted. Worth verifying
  the test's depth and extending if shallow.
- **[P2] Quick-travel buttons** are listed in features.md but no
  visible test references them. Suggested: extend `InputField.test.ts`
  or `Sidebar.test.ts` to assert click-to-navigate.

## 5. Recommendations

1. **Cover `SavePicker.svelte` and `DebugPanel.svelte` first.** Both
   are complex, user-facing, currently untested at the component
   level, and gate critical UX (saves, debugging). Highest single
   leverage in the UI surface.
2. **Add a smoke test per editor component.** Even one render+click
   per file would close 6 silent-regression vectors in the mod
   editor.
3. **Add a `theme.test.ts` covering time/season/weather inputs to
   color outputs** — pixel regression covers the rendered result but
   not the function purity.
4. **Verify `/irish`, `/improv`, and tile-source switching** through
   either UI unit or E2E paths; they are user-visible toggles with
   no current sensor.
