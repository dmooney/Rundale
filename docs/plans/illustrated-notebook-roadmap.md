# Roadmap: Illustrated Notebook UI

Status: Integrated default play surface complete; deferred follow-on slices are
tracked with their residual risks below.

This is the source of truth for finishing the Rundale illustrated notebook play
surface. GitHub issues should track executable slices; this document preserves
the shared direction, acceptance bar, and ordering.

## North Star

The default Rundale play screen should read as an illustrated notebook game:
full-bleed storybook scene, in-world markers, parchment UI, sketched people,
stamp actions, and a handwritten command line. Svelte remains the host shell and
backend bridge, but the first viewport stays Pixi-rendered and asset-driven.

Non-negotiables:

- No visible old dashboard chrome, persistent columns, old map panel, mobile
  toolbar, or debug toolbar in the first viewport.
- No visible `InputField.svelte`, `.input-wrapper`, `.input-form`, rectangular
  text box, or old SEND button in the first viewport.
- Runtime notebook UI combines bitmap plates/cutouts with fine Pixi-drawn
  ink/parchment controls, not dashboard-style CSS boxes.
- The right-hand page is hand-sewn; binding rings, ring holes, and paperclips are
  outside the concept.
- Backgrounds and runtime visual-scene metadata come from written descriptions,
  not historical map image references.
- Prompts/metadata use wide elevated oblique storybook language, not strict
  isometric/isomorphic requirements.
- Desktop and mobile screenshots are required for every visible slice.

## Current Baseline

The fresh #1630 baseline adds:

- A PixiJS play surface hosted by
  `IllustratedNotebookGame.svelte`.
- A new renderer/layout/interaction boundary under
  `parish/apps/ui/src/lib/illustrated-parish/`; it does not inherit the rejected
  `src/lib/illustrated-notebook/` visual implementation.
- A fresh asset kit under
  `parish/apps/ui/static/rundale/illustrated-notebook-v2/`: desktop/mobile parish
  plates, concept-referenced raster parchment/ink cutouts, approved person
  portraits and markers, and the explicitly approved hand-sewn page. The kit
  does not import either rejected `notebook-ui` asset set.
- Concept-aligned scene people and labels, Nearby rail, sewn notebook page,
  contiguous action strip, command strip, Map/Time cards, and Active Intents
  card.
- A hidden native input for keyboard/accessibility with Pixi-rendered visible
  command treatment, including Enter/send/stamp submission, busy/error states,
  and session command-history navigation.
- A single notebook-styled overlay coordinator/host for Journal, People,
  Focail, Map, Save/Load, Debug, Mod, Bug Report, and supporting utility
  surfaces.
- Visual-scene metadata/tests rejecting historical map reference language and
  strict isometric/isomorphic runtime requirements, plus screenshot, selector,
  and asset-provenance regression coverage.

Deferred follow-on slices:

- **#1631 — location scene metadata and anchors.** Deferred until the next
  location-expansion product slot is explicitly scheduled. Residual risk: beyond
  the starter loop, shared fallback anchors can weaken scene-specific placement.
- **#1632 — mobile layout hardening.** Deferred until a device-matrix hardening
  slot is explicitly scheduled. Residual risk: uncommon short or extreme mobile
  viewports have less focused layout coverage than the proven 390x844 surface.
- **#1633 — visual regression and provenance gates.** Deferred until the
  cross-slice visual-quality slot is explicitly scheduled. Residual risk:
  notebook-specific safeguards remain distributed across the shipped slice tests
  and provenance checks rather than one coordinator-owned gate.

## Epic And Slice Issues

Create one epic issue named:

- `Epic: finish the illustrated notebook game UI`

Create these vertical-slice issues and link them from the epic:

1. `Pixi notebook interactions: hit targets, hover/focus, and routing`
2. `Notebook person and marker art system`
3. `Notebook command strip v2`
4. `Notebook secondary overlays and drawers`
5. `Rundale location scene metadata and anchor pipeline`
6. `Notebook mobile layout hardening`
7. `Notebook visual regression and asset provenance gates`

Each slice should be implemented as a complete user-visible step: art/data,
renderer behavior, interaction wiring, tests, and fresh screenshots in one PR.
Do not split pure art from the system that consumes it unless the art is already
independently usable and documented.

## Slice Acceptance Criteria

### 1. Pixi notebook interactions

- All visible first-viewport controls have Pixi hit targets and cursor/hover
  states: NPC markers, Nearby portraits, notebook tabs, action stamps, send
  affordance, Map/Time cards, and Active Intents card.
- Clicks route through existing stores/overlay state; no old persistent dashboard
  panels return to the first viewport.
- Keyboard focus has a visible notebook-native treatment where applicable.
- Tests cover hit target dispatch and default selected-person behavior.
- Desktop/mobile screenshots prove the first viewport remains notebook-native.

### 2. Person and marker art system

- Extend the four initial notebook portrait cutouts to the Rundale cast shown in
  the Nearby rail/notebook page.
- Keep scene people readable through depth/placement rules and a thin selection
  callout, without geometric body-marker chrome.
- Add manifest/provenance entries for portraits and markers.
- Tests verify fallback behavior for NPCs without approved art.
- Screenshots prove people read as sketched notebook/game assets, not web icons.

### 3. Command strip v2

- Visible command text, caret, focus, placeholder, busy, disabled, and error
  states remain Pixi/notebook-rendered.
- Enter submits, stamp buttons seed commands, send affordance submits, and input
  clears/focuses predictably through existing `submitInput`.
- Add command history navigation.
- Decide separately whether mentions/slash autocomplete returns in this slice;
  if it does, it must use notebook-native rendering, not `InputField.svelte`.
- Tests cover submit, history, busy/error, seeded actions, and absence of old
  input chrome.

### 4. Secondary overlays and drawers

- Journal/chat, People, Focail, Map, Save/Load, Debug, Mod, and Bug Report open
  from notebook tabs/cards/shortcuts as drawers or modal overlays.
- Overlay wrappers match the notebook art direction and visually isolate any
  reused legacy Svelte internals.
- Closing an overlay restores the Pixi first viewport without layout shift.
- Tests cover each routing entry point.
- Screenshots include at least one representative overlay on desktop and mobile.

### 5. Location scene metadata and anchor pipeline

- Add per-location scene metadata for the first playable village loop:
  Kilteevan Village, The Forge, The Holy Well, The Mill, The Weaver's Cottage,
  St. Brigid's Church, Murphy's Farm, The Lime Kiln, The Letter Office, and The
  Crossroads.
- Each location has a written visual summary, plate asset path, camera hint,
  player/NPC/exit anchors, and depth bands.
- Metadata tests continue to reject historical map reference language and strict
  isometric/isomorphic requirements.
- Renderer falls back gracefully when metadata or plate assets are missing.
- Screenshots prove at least three different locations use distinct anchors.

### 6. Mobile layout hardening

- 390x844 keeps the same notebook language without old mobile toolbar/dashboard
  chrome.
- Nearby rail, notebook page, markers, action stamps, and command strip avoid
  overlap and clipped labels.
- Mobile can intentionally collapse Map/Time and Active Intents, but the route
  to those surfaces remains available.
- Add layout tests for narrow, short, and tall mobile viewports.
- Capture fresh mobile screenshots for every mobile layout change.

### 7. Visual regression and provenance gates

- Add a repeatable screenshot capture/check path for the notebook first
  viewport.
- Add selector regression checks for legacy first-viewport chrome.
- Add asset manifest/provenance checks so runtime assets are documented and not
  concept-image slices.
- Reject imports from visual namespaces/assets that a fresh rebuild explicitly
  excluded.
- Add visual-scene metadata checks for banned source-map/projection language.
- Document how to regenerate or replace each asset class.

## Definition Of Done

Every notebook UI PR must include:

- Focused unit/component tests for behavior touched.
- Relevant `parish-world` tests if scene metadata or prompt/provenance rules
  change.
- `fnm exec --using 22 npm run check`, `lint`, `format:check`, and `build` when
  frontend code changes.
- Fresh desktop and mobile screenshots from the running app.
- `just agent-check` with a local proof bundle when runtime-shipping files
  change.
- A PR body that states whether old `InputField.svelte` and dashboard chrome are
  absent from the first viewport.

## Issue Labels

Use:

- Epic: `epic`, `frontend`, `ux`, `P1`
- Slices: `frontend`, `ux`, `enhancement`, `P1`
- Asset/provenance-heavy slices may also use `documentation` when docs change.

Avoid `bug` unless a slice fixes a shipped regression rather than completing the
planned notebook UI.
