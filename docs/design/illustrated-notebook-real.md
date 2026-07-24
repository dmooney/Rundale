# Illustrated Notebook Real Play Screen

Status: Implemented foundation; refreshed for issue #1630

## Player Experience

Rundale opens directly into a full-screen illustrated game scene that follows
the named
[notebook concept](../graphics-v2/illustrated-parish-notebook.png), not a web
dashboard. The player sees a watercolor parish plate with in-world labels and
people, a parchment ribbon, nearby portrait strip, right-hand hand-sewn notebook
page, action strip, and handwritten intent strip. The approved sewn page has no
ring binding or paperclip. Existing Svelte components may still power secondary
overlays, but the default play surface is a Pixi-rendered notebook interface.

## Affected Subsystems

- `parish/apps/ui/src/routes/+page.svelte`: keep page lifecycle, global
  shortcuts, screenshot/setup, and controller setup while mounting the Pixi
  play surface and one overlay host.
- `parish/apps/ui/src/components/illustrated-notebook/`: Svelte canvas host,
  accessibility input, and notebook-styled overlay host.
- `parish/apps/ui/src/lib/illustrated-parish/`: fresh renderer, responsive
  layout, asset manifest, interaction routing, types, and tests. This namespace
  is the visual implementation boundary for the #1630 rebuild.
- `parish/apps/ui/src/stores/notebookOverlay.ts`: canonical routing and focus
  restoration for notebook overlays.
- `parish/apps/ui/src/stores/game.ts`: read existing stores only; do not fork
  transport or create parallel state ownership.
- `parish/apps/ui/src/lib/ipc.ts`: submit commands through existing `submitInput`.
- `parish/apps/ui/static/rundale/illustrated-notebook-v2/`: fresh runtime scene
  plates, temporary people-layout stand-ins, and the explicitly approved
  hand-sewn page. Portrait-system work remains separate.
- `parish/apps/ui/static/rundale/illustrated-notebook-v2/visual-scenes.json`:
  fresh plate paths, written visual summary, camera hint, anchors, and depth
  bands kept inside the same provenance boundary as the runtime art.
- `parish/crates/parish-world`: validation for visual-scene prompt/metadata
  language that rejects historical-map-reference dependencies and strict
  isometric/isomorphic requirements.

No Rust gameplay behavior changes are intended. No new engine/gameplay feature
flag is required because this replaces the default frontend presentation, not a
new gameplay rule.

## Data Model

Frontend rendering derives from existing state:

- Current location/time/weather from `worldState` and map/world snapshots.
- Nearby people from `npcsHere`.
- Exits and map context from `mapData`.
- Busy/streaming/error state from existing game stores.
- Command submission from `submitInput`.

New frontend-only models:

- `ParishLayout`: responsive rectangles and scene anchors for the concept's
  desktop and mobile composition.
- `ParishHitTarget`: ordered hit regions and semantic activations for portraits,
  tabs, actions, intent, cards, and overflow controls.
- `ParishRenderState`: the existing world/map/NPC state presented to Pixi.
- `NotebookSurface`: the canonical journal, people, Focail, map, save, debug,
  mod, bug, shortcuts, utility, time, intents, and rumours overlay routes.

Potential mod metadata is additive. It must not break existing saves and must
not require coordinate edits.

## Runtime Art Assets

The clean runtime asset kit lives under
`parish/apps/ui/static/rundale/illustrated-notebook-v2/`. It contains:

- `parish-crossroads-watercolor.png` and
  `parish-crossroads-watercolor-mobile.png`: fresh desktop and vertical scene
  plates.
- `parchment-*.png`: fresh transparent top-ribbon, Nearby-rail, action-strip,
  intent-strip, tab, label, and bottom-card cutouts generated from the canonical
  concept's paper language.
- `icon-*.png`: fresh transparent action, map, time, and quill cutouts generated
  from the concept's loose charcoal/sepia symbols.
- `portrait-slot-frame.png`: an intentionally empty raster frame. Runtime
  initials reserve the Nearby and selected-person slots; portrait art and the
  portrait/fallback system remain outside issue #1630.
- `sewn-notebook-page.png`: the explicitly approved hand-sewn notebook page.
- `ui-assets.json`: dimensions, alpha contracts, provenance classes, and hashes
  for every runtime image.

Pixi preloads the raster parchment and ink cutouts, then draws dynamic text,
trust dots, hit regions, focus treatments, and selection callouts at runtime.
There are deliberately no binding rings, ring holes, or paperclip. The renderer
must not import either rejected `notebook-ui` visual kit or the rejected
`src/lib/illustrated-notebook/` implementation. The concept image is a named
style/composition reference, never a runtime image slice.

## Scene Plate And Prompt Rules

The runtime scene plate should use written scene descriptions only. Historical
maps may not be used as runtime image references for this pass.

Prompt/metadata language should prefer:

> wide elevated oblique illustrated storybook game scene

Do not require strict isometric or isomorphic projection. The renderer maps its
current people and exit anchors through the plate's actual cover crop. The
metadata depth bands remain available to the separate person/marker slice.

Add tests in `parish-world` or the nearest existing visual-scene validation
module to reject:

- `historical map`, `map crop`, `NLS`, `Ordnance Survey`, or equivalent source
  map-reference dependencies in runtime prompts/metadata.
- strict `isometric` or `isomorphic` requirement language in new runtime prompt
  fields.

The tests may document old rejected experiments only outside runtime metadata.

## Pixi Rendering Layers

The renderer owns the visible first viewport:

1. background scene plate
2. subtle scene wash/vignette if needed
3. in-world exit labels
4. scene people and labels
5. thin selection ellipse/callout
6. top parchment ribbon
7. left Nearby portrait strip
8. right notebook page/tabs
9. bottom action stamps
10. bottom intent command strip
11. Map/Time cards
12. Active Intents card

Svelte should host the canvas, subscribe to stores, pass render props into the
renderer, submit commands, and open secondary overlays only when requested.

## Interaction Model

The sewn page and its protruding tabs are one persistent notebook surface:

- **Notes** records the current scene, conditions, and next-action guidance.
- **People** shows the selected person's record and the nearby directory.
- **Places** is a written directory of the current and adjacent places.
- **Rumours** holds learned stories.
- **Journal** shows recent narrative and conversation entries.

Turning a tab changes that page in place; it does not open a dialog. Selecting a
person also turns to People. The separate **Map** card opens a notebook-styled
geographic sheet with routes and zoom/pan controls, while utility and
interruptive work such as Save/Load, Debug, Mod, Bug Report, and Shortcuts uses
dismissible notebook-styled sheets. Closing a sheet returns to the same tab,
scene, command draft, canvas dimensions, and invoking control.

## Command Input

The visible input must be new. Preferred implementation:

- Pixi renders parchment strip, handwritten line, placeholder, focus/caret
  affordance, seeded text, busy state, and ink/stamp send affordance.
- A hidden or minimally styled native input/textarea handles keyboard,
  selection, IME, screen-reader label, and clipboard behavior.
- Enter submits natural-language text through existing `submitInput`.
- Action stamps seed intent text and focus the hidden input.
- Existing error handling is reused.

Mentions, slash-command autocomplete, model autocomplete, and tab-complete
dropdowns are not required in the first Pixi slice; keeping them out is
intentional so the old `InputField.svelte` visual treatment does not leak into
the default viewport.

## Observable Signals

- `cargo run -p parish-engine -- --script
testing/fixtures/play_illustrated-notebook-real.txt` proves the backend
  behavior path still returns status, scene, NPCs, map, time, natural-language
  command handling, and movement.
- Unit tests prove target ordering, layout/crop mapping, and command input
  submit/stream-flush behavior.
- `parish-world` tests prove prompt/metadata language rejects historical map
  dependencies and strict isometric/isomorphic requirements.
- Desktop and mobile screenshots prove the first viewport meets the notebook
  concept and lacks the old dashboard/InputField treatment.
