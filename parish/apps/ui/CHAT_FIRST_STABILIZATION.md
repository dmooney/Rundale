# Chat-First Stabilization Contract

Status: implementation and final live audit complete  
Baseline: `origin/main` at `af6f2b8f`  
Decision owner: this migration  
Experiment disposition: remove the illustrated-notebook renderer after retaining
approved art and provenance; do not maintain a second player shell.

This is a forward stabilization migration. It does not restore a historical
route wholesale.

## Bounded baseline audit

The pre-change audit covers one desktop viewport and one mobile viewport:

| Player task                          | Desktop baseline                                                                                                | Mobile baseline                                                                                                                | Evidence after change                     |
| ------------------------------------ | --------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------- |
| Command entry and completion         | The only native input is a 1×1 off-screen `Player intent` control; the visible command strip is canvas-rendered | Canvas-strip entry submits, but the native input remains 1×1 and retains focus after submission                                | Screenshot + semantic Playwright          |
| Transcript readability and streaming | The first viewport has no readable transcript; recent dialogue requires changing notebook sections              | The full-height viewport has no transcript; accessible fallback text exposes scene/status only                                 | Screenshot + semantic Playwright          |
| NPC addressing and reactions         | Nearby art/action stamps are visible, but mature mention/completion controls are absent                         | One Nearby portrait is visible; the selected-person status exists only in accessibility text                                   | Screenshot + semantic Playwright          |
| Map                                  | Map card and sheet are present; selected suite passes the MapLibre destination                                  | After the hidden input gains focus, pressing M appends `M` to the intent instead of opening map, even after clicking the scene | Screenshot + shortcut/focus Playwright    |
| Save/load                            | Selected save contracts pass                                                                                    | F5 opens a usable full-width ledger and Escape closes it                                                                       | Screenshot + Playwright                   |
| Debug and bug report                 | Selected debug contracts pass                                                                                   | F12 opens records; the horizontal tab rail clips later tabs but remains scrollable                                             | Screenshot + Playwright                   |
| Keyboard navigation and focus return | Keyboard input depends on a 1×1 native control mirrored into Pixi                                               | Focus remains on that hidden input across scene clicks, making single-letter shortcuts unavailable once entry starts           | Playwright focus assertions               |
| Resize and safe-area behavior        | 1,440×900 fits without body overflow                                                                            | 390×844 settles without body overflow, but an early capture is blank while Pixi textures load                                  | Screenshot + Playwright                   |
| Screenshot capture                   | Selected mocked screenshot contracts pass                                                                       | Live web F2 reports `501 Not Implemented`; graphical-readiness calls also log `404 Not Found`                                  | Captured-file inspection in web and Tauri |
| Setup, demo, and editor return       | Selected editor-return contracts pass; setup/demo not manually exercised                                        | Not manually exercised in the bounded run                                                                                      | Playwright                                |

Baseline evidence is stored in the ignored proof bundle as
`baseline-desktop.png` and `baseline-mobile.png`. The live audit used the real
`parish-server` at 1,440×900 and 390×844. The selected baseline Playwright run
passed 52 tests with 2 intentional skips across `app`, `features`,
`interactions`, `smoke`, and `screenshots`.

The audit is deliberately bounded to this matrix. It is evidence gathering, not
another repair attempt on the notebook.

## Current interaction-surface producers

The structural graph and source search identify these production producers:

| Producer                                                     | Current route                                                                                  | Required migration                                                                               |
| ------------------------------------------------------------ | ---------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| `routes/+page.svelte`                                        | Keyboard shortcuts, screenshot toast, debug polling, controller lifecycle, notebook host       | Render `ChatGameShell`; route shortcuts through `SurfaceCoordinator`; retain lifecycle and toast |
| `lib/page-controller.ts`                                     | Required-mod open, backend map toggle, backend save open                                       | Depend on presentation-neutral coordinator                                                       |
| `components/StatusBar.svelte`                                | Player-facing status and utility entry points                                                  | Route utilities through coordinator                                                              |
| `components/BugChip.svelte`                                  | Prepares and opens contextual bug report                                                       | Route bug surface through coordinator without losing context                                     |
| `components/illustrated-notebook/NotebookOverlayHost.svelte` | Synchronizes coordinator and legacy visibility stores; renders every secondary surface         | Replace with presentation-neutral `SurfaceHost`                                                  |
| `stores/notebookOverlay.ts`                                  | Exclusivity, blocking, transitions, bug preparation, focus restoration, legacy synchronization | Rename/retype as `surfaceCoordinator.ts`; preserve invariants                                    |
| `lib/screenshot.ts` and `lib/ipc/screenshot.ts`              | Browser DOM capture plus native/backend screenshot requests                                    | Capture chat shell and active dialogs; remove canvas preference                                  |
| Tauri/server IPC event producers                             | `onSavePicker`, `onToggleFullMap`, debug updates, screenshot requests                          | Keep event contracts and give each a visible chat-shell destination                              |

Legacy visibility stores remain adapter state only while their components still
consume them: `fullMapOpen`, `savePickerVisible`, `modSelectorVisible`,
`debugVisible`, and `bugReportVisible`. New producers must not write them
directly.

## Coordinator invariants

`SurfaceCoordinator` owns the active surface:

`map | save | debug | mod | bug | shortcuts | null`

It must preserve:

- One active surface and one transition at a time.
- Required-mod blocking.
- Asynchronous bug-report preparation and cancellation.
- Invoker capture and focus restoration.
- Escape and same-surface toggle behavior.
- Backend-triggered map/save destinations.
- Temporary synchronization with legacy visibility stores.
- Safe adoption of a legacy close until direct store writes are removed.

The coordinator API and types must contain no notebook/Pixi terminology.

## Chat interaction models

### Desktop

- `StatusBar` remains the top-level status and utility entry point.
- The main region is a readable `ChatPanel`.
- `InputField` is always reachable below the transcript and retains mention,
  slash-command, model, history, and adjacent-location completion.
- `Sidebar` remains visible beside chat when width permits and exposes nearby
  people and language hints.
- Map, save, debug, bug report, shortcuts, setup, and required-mod selection are
  modal or sheet surfaces coordinated above the shell.
- Closing a surface returns focus to its invoker or the input.

### Mobile

- `StatusBar` remains compact and does not push the input below the viewport.
- Chat is the primary pane.
- Nearby people, language hints, and other sidebar information are reachable
  through an explicit mobile control, not silently omitted.
- The input remains above the safe-area inset and is not covered by sheets or
  the software keyboard.
- Secondary surfaces use the full available width, trap focus while open, and
  restore the prior chat/mobile state when closed.

## Capability disposition

| Capability                                                              | Disposition                                                                                                              |
| ----------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| Command submission, streaming, transcript, reactions                    | Preserve in chat                                                                                                         |
| NPC addressing, slash/model/location completion, history                | Preserve through `InputField`                                                                                            |
| Nearby NPCs and language hints                                          | Preserve through responsive `Sidebar`                                                                                    |
| Notes, People, Places, Rumours, Journal tabs                            | Retire as notebook navigation; retain underlying player information only where it already has a chat/sidebar destination |
| Person selection and action seeding                                     | Preserve NPC addressing; retire notebook-only action stamps                                                              |
| Map, save/load, debug, bug report, shortcuts                            | Move to presentation-neutral coordinated surfaces                                                                        |
| Setup, demo, required-mod selection, editor                             | Preserve                                                                                                                 |
| Pixi scene hit targets, notebook page turns, raster tabs, command strip | Retire                                                                                                                   |

## Browser-contract migration matrix

| Spec                                          | Disposition                         | Replacement contract                                                                                                                   |
| --------------------------------------------- | ----------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| `app.spec.ts`                                 | Migrate                             | Chat shell composition, status data, NPC/sidebar content, utilities, theme, and text-log/world-update behavior                         |
| `bug-report-hotkey.spec.ts`                   | Preserve                            | Text-entry shortcut isolation and map shortcut outside inputs                                                                          |
| `chat-feed-rendering.spec.ts`                 | Preserve                            | Destination bubbles, reaction wrapping, and proof screenshot in chat                                                                   |
| `features.spec.ts`                            | Migrate                             | Debug, save/load, hints, reactions, editor return, and coordinated close/reopen behavior                                               |
| `illustrated-notebook-command-states.spec.ts` | Retire implementation assertions    | Input enabled/streaming/paused/inference state moves to semantic `InputField` assertions                                               |
| `illustrated-notebook-interactions.spec.ts`   | Split                               | Retire page turns, canvas geometry, and notebook-native states; migrate history, transitions, mobile navigation, and tool destinations |
| `interactions.spec.ts`                        | Preserve/migrate selectors          | Submission, flush-on-interaction, mid-chain loading, multi-NPC streams, pause, inference pause, and festival status                    |
| `local-inference-fork.spec.ts`                | Preserve                            | Setup fork and progress behavior                                                                                                       |
| `notebook-person-art-proof.spec.ts`           | Move out of default-route suite     | Keep provenance/decoder catalog validation; add chat portrait checks only after portrait promotion                                     |
| `notebook-raster-icons-proof.spec.ts`         | Retire shipped-surface pixel probes | Static catalog/provenance validation only for retained assets                                                                          |
| `scene-dedup.spec.ts`                         | Preserve                            | Arrival deduplication and load restoration in transcript                                                                               |
| `screenshots.spec.ts`                         | Regenerate                          | Bare chat and incremental-art desktop/mobile baselines                                                                                 |
| `slash-command-echo.spec.ts`                  | Preserve                            | Slash echo ordering in chat                                                                                                            |
| `smoke.spec.ts`                               | Migrate selectors                   | Page state, command, movement, API, and screenshots through chat                                                                       |

No existing behavioral assertion may be deleted without landing in this table or
an updated per-assertion appendix.

## Asset disposition

| Asset class                        | Current evidence                                                                     | Disposition                                                                                                          |
| ---------------------------------- | ------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------- |
| Scene plates                       | `illustrated-notebook-v2/parish-crossroads-watercolor*.png`, runtime scene manifests | Retain as candidates for the first post-stabilization responsive scene header                                        |
| NPC portraits                      | `notebook-ui/people/portrait-*.png` with contact sheet and provenance                | Retain; promote only after scene-header checkpoint                                                                   |
| NPC map markers                    | `notebook-ui/people/marker-*.png`                                                    | Preserve with provenance; use only if a DOM map/player task demonstrates value                                       |
| Parchment frames and utility icons | 24 notebook-v2 UI PNGs plus manifests                                                | Preserve initially; promote selected assets only after portraits; delete unused runtime copies at experiment closure |
| Notebook/Pixi-only layout assets   | Sewn page, index rail, tab/action-strip composition assets                           | Candidate deletion when renderer removal proves no remaining import                                                  |
| Graphics research corpus           | `docs/graphics-v2` (1,356 files at baseline)                                         | Preserve as immutable provenance/source material; never ship solely because it exists                                |
| Production art metadata            | runtime READMEs, manifests, contact sheet, provenance                                | Retain and update to record chat use or archival-only status                                                         |

## Ordered verification checkpoints

1. Capture baseline desktop/mobile evidence and finish this audit table.
2. Land and test `SurfaceCoordinator` without changing the default renderer.
3. Switch to bare `ChatGameShell`; pass semantic unit, type, and Playwright
   contracts before adding art.
4. Add one responsive scene plate and recapture desktop/mobile evidence.
5. Add portraits, then only selected frames/icons, with a passing checkpoint
   after each asset class.
6. Remove renderer, Pixi adapters, notebook-only tests, and unused dependency;
   verify no production import or documentation default remains.
7. Run the full verification and live proof audit against the acceptance
   criteria.

## Assertion-level migration appendix

The pre-change browser suite was reviewed assertion by assertion and grouped by
the player contract each assertion represented:

| Prior assertions                                                                                                                                                                                       | Final disposition                                                                                                                                                                            |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Default route contains a Pixi canvas, illustrated notebook host, page tabs, command strip, hidden native intent, action stamps, raster geometry/pixel probes, and no chat/sidebar/map/input components | Retired. These asserted the removed implementation and, in several cases, the absence of the restored player interaction model.                                                              |
| Default route loads world state, time/weather/season/festival, nearby NPCs, language hints, map context, and theme                                                                                     | Migrated to `app.spec.ts`, `interactions.spec.ts`, and the chat visual baselines using semantic status/sidebar/map assertions.                                                               |
| Plain submission, addressed submission, input history, mention/slash/model/location completion, adjacent travel, draft protection, pause/resume, and streaming input state                             | Preserved in the existing `InputField` unit suite; plain submission, travel invocation, accessible busy state, and text-entry shortcut isolation also run in semantic Playwright.            |
| Streaming token ordering, flush-on-interaction, mid-chain `loading=false`, overlapping NPC speakers, loading state, transcript attribution, sticky scroll, and auto-scroll after player submission     | Preserved in `ChatPanel`/controller unit tests and migrated semantic Playwright in `interactions.spec.ts`.                                                                                   |
| Player/NPC/system rendering, slash-command ordering, movement destination legibility, reaction wrapping/picker, scene deduplication, and load restoration                                              | Migrated to the visible `ChatPanel` in `chat-feed-rendering.spec.ts`, `slash-command-echo.spec.ts`, and `scene-dedup.spec.ts`.                                                               |
| Notebook tab navigation for Notes, People, Places, Rumours, and Journal; page turns; action-seed stamps; More/Time/Active Intents sheets                                                               | Retired as notebook-only navigation. Underlying chat, nearby people, language hints, map, time/status, and tool destinations remain visible through the chat shell.                          |
| Map, Ledger, Debug, Mod, Bug Report, shortcuts, required-mod blocking, exclusivity, backend map/save events, Escape, focus restoration, and shortcut isolation                                         | Migrated to `SurfaceCoordinator` unit tests plus `app.spec.ts`, `features.spec.ts`, and `bug-report-hotkey.spec.ts`.                                                                         |
| Setup fork/progress, demo, editor tabs/return, screenshot invocation, controller mount/reconnect/disposal, and API smoke                                                                               | Preserved. Setup remains in `local-inference-fork.spec.ts`; editor return and screenshot-safe surfaces are semantic Playwright contracts; controller/lifecycle behavior remains unit-tested. |
| Notebook texture readiness, preserved WebGL buffer, canvas capture, notebook-only screenshots, and default-route person-art pixel decoding                                                             | Retired. DOM screenshot targeting is unit-tested, chat desktop/mobile baselines replace notebook screenshots, and approved portrait provenance remains in the deterministic asset pipeline.  |

## Implementation result

- `SurfaceCoordinator` is the presentation-neutral authority for map, save,
  debug, mod, bug-report, and shortcuts surfaces. The page controller,
  `StatusBar`, `BugChip`, and global shortcuts use that API.
- `ChatGameShell` is the default route and composes the mature status, chat,
  input, map, sidebar, and responsive mobile controls.
- `SurfaceHost` preserves exclusivity, required-mod blocking, bug preparation
  and cancellation, legacy-store synchronization, focus trapping, and focus
  return.
- DOM screenshot capture targets the complete app root, including an active
  coordinated surface, with chat-shell and body fallbacks.
- The Pixi renderer, canvas host, renderer adapters, notebook coordinator,
  notebook command strip, notebook-only unit/E2E contracts, and `pixi.js`
  dependency were removed.
- The bare chat checkpoint passed focused unit tests, production build, and 16
  semantic desktop/mobile Playwright contracts before art promotion.
- Art promotion followed the planned order: responsive scene plate
  (`SceneHeader`), approved portrait images (`Sidebar`), then the selected map
  icon. Each class is rendered as ordinary DOM images and is covered by unit or
  semantic browser assertions.
- New desktop morning/midday/dusk/night and 390×844 mobile visual baselines show
  the readable chat shell with reachable input and no horizontal overflow.
