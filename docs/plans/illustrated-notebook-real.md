# Plan: Illustrated Notebook Real Play Screen

Status: Retired historical plan

The renderer and interaction architecture described below were removed by the
chat-first stabilization migration. Approved scene and portrait assets were
retained for responsive DOM use.

This implementation plan follows the canonical
[`illustrated-parish-notebook.png`](../graphics-v2/illustrated-parish-notebook.png)
concept and the active
[`illustrated-notebook-roadmap.md`](illustrated-notebook-roadmap.md). The first
implementation attempt is rejected as a visual source: its renderer, layout,
asset kit, and proof artifacts are not inputs to this rebuild.

## Clean Boundary

- The active visual implementation lives in
  `parish/apps/ui/src/lib/illustrated-parish/`.
- Runtime art lives in
  `parish/apps/ui/static/rundale/illustrated-notebook-v2/`.
- The approved `sewn-notebook-page.png` is the sole retained visual exception.
  Its 440×620 shape is preserved without stretching.
- The rejected `src/lib/illustrated-notebook/` visual modules and
  `static/rundale/notebook-ui/` assets were removed. The existing pure command
  helpers remain shared behavior, not visual provenance.
- The page is hand-sewn. Spiral binding, rings, ring holes, and paperclips are
  outside the art direction and the 1820 setting.
- Portrait-system expansion belongs to the separate person-and-marker slice;
  issue #1630 does not broaden that system.

## Current Implementation Slices

### 1. Fresh Pixi parish surface

1. Host one full-viewport Pixi canvas from
   `IllustratedNotebookGame.svelte`.
2. Compose the desktop/mobile watercolor parish plates with fine ink and muted
   parchment treatments that track the canonical concept's placements.
3. Render the top ribbon, Nearby rail, sewn notebook page and tabs, action
   strip, handwritten intent strip, and bottom cards from existing game state.
4. Keep the old status bar, persistent chat/sidebar/map, dashboard input, and
   developer chrome out of the default viewport.

### 2. Input and accessible controls

1. Render the visible intent treatment in Pixi while a minimal native input
   owns keyboard, clipboard, IME, and screen-reader behavior.
2. Submit through the existing `submitInput` contract; action stamps seed the
   intent and Enter submits.
3. Mirror Pixi hit regions with ordered semantic controls and a visible
   notebook-native focus treatment.
4. Keep mentions, slash autocomplete, history, and multiline editing deferred
   until they have notebook-native presentation.

### 3. Secondary overlays — issue #1630

1. Route Journal/chat, People, Focail, Map, Save/Load, Debug, Mod, and Bug
   Report from tabs, cards, the More sheet, or global shortcuts.
2. Mount reused Svelte interiors inside one notebook-styled overlay host so
   their fixed dashboard chrome cannot move or redefine the Pixi viewport.
3. Keep only one secondary surface active, trap focus inside it, and restore
   focus to the originating notebook control when it closes.
4. Keep the same Pixi host mounted and inert beneath an overlay; closing must
   restore identical canvas bounds.
5. Keep required Mod selection non-dismissible and capture Bug Report evidence
   only after the current sheet has left the viewport.

### 4. Provenance and regression guards

1. Document every runtime asset in the v2 asset README and `ui-assets.json`,
   including dimensions, alpha contract, provenance, and hash.
2. Reject active imports from the discarded visual namespace and asset kit.
3. Pin the approved sewn-page hash in a test.
4. Test the canonical desktop placements, mobile composition, cover-crop
   annotation mapping, page aspect ratio, and tab bounds.
5. Keep visual-scene prompt metadata grounded in written descriptions rather
   than historical-map crops or strict isometric projection requirements.

## Verification

1. Run frontend format, lint, Svelte diagnostics, unit tests, and production
   build on the repository's locked package versions.
2. Run the Rust quality gates and the real engine walkthrough.
3. Capture fresh 1440×900 desktop and 390×844 mobile first viewports from the
   running UI, plus a representative overlay at each size.
4. Compare those captures directly with the canonical concept; do not reuse
   screenshots from the rejected attempt.
5. Record runtime proof in `.proofs/1630/`, map every acceptance criterion in
   `evidence.md`, add an independent judge verdict, and run `just agent-check`.

## Rework Triggers

- The first viewport still reads as the rejected UI with a new asset swapped
  into it.
- A legacy dashboard surface is visible before the player opens it.
- The sewn page is stretched, replaced with a ring-bound page, or decorated
  with a paperclip.
- A routed overlay changes or remounts the Pixi viewport.
- Active visual code imports the rejected renderer/layout/assets.
- Desktop or mobile proof was not freshly captured and compared with the named
  concept.

Any rework trigger is a requirement failure, not future polish.
