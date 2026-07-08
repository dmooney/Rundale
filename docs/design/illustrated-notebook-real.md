# Illustrated Notebook Real Play Screen

Status: Pre-implementation design note

## Player Experience

Rundale opens directly into a full-screen illustrated game scene that looks like
the approved notebook concept, not a web dashboard. The player sees a
watercolor parish plate with in-world labels and markers, a parchment ribbon,
nearby portrait strip, right-hand spiral notebook page, action stamps, and a
handwritten intent strip. The old Svelte dashboard components may still power
secondary overlays, but the default play surface is a Pixi-rendered notebook
interface.

## Affected Subsystems

- `parish/apps/ui/src/routes/+page.svelte`: keep page lifecycle, global
  shortcuts, screenshot/setup/save/debug overlays, and controller setup, but
  mount the Pixi play surface for the default viewport.
- `parish/apps/ui/src/components/illustrated-notebook/`: new Svelte host
  component and any accessibility overlays for the Pixi canvas.
- `parish/apps/ui/src/lib/illustrated-notebook/`: renderer, layout model, asset
  manifest, marker depth-scale helpers, command input controller, and tests.
- `parish/apps/ui/src/stores/game.ts`: read existing stores only; do not fork
  transport or create parallel state ownership.
- `parish/apps/ui/src/lib/ipc.ts`: submit commands through existing `submitInput`.
- `parish/apps/ui/static/rundale/notebook-ui/`: original runtime bitmap asset
  kit and provenance notes.
- `mods/rundale/`: optional visual-scene metadata that points to the approved
  runtime plate, written visual summary, camera hint, anchors, and depth bands.
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

- `NotebookSceneAnchor`: normalized `{ id, x, y, depth, kind }` anchors for
  player, nearby NPCs, exits, and labels.
- `NotebookDepthBand`: `{ minDepth, maxDepth, minScale, maxScale }` bands used
  to keep far markers readable without pretending the scene is strict
  isometric.
- `NotebookAssetManifest`: paths for the generated/original bitmap UI assets.
- `NotebookSelection`: selected NPC real name, defaulting to the selected/current
  NPC if known, otherwise the first nearby NPC.

Potential mod metadata is additive. It must not break existing saves and must
not require coordinate edits.

## Runtime Art Assets

Create a clean asset kit under `parish/apps/ui/static/rundale/notebook-ui/`.
Do not cut up `docs/graphics-v2/illustrated-parish-notebook.png`.

Required assets:

- parchment top ribbon
- spiral notebook page
- notebook binding/rings
- side tab stack
- bottom intent parchment strip
- handwritten input line texture
- ink/stamp send affordance
- action stamp buttons/icons for Talk, Ask, Help, Observe, Leave
- Nearby portrait card frame
- sketched portrait placeholders or generated portrait set
- Active Intents card
- Map card
- Time card
- paper exit label
- NPC selection ring
- player marker
- NPC silhouette/marker set with depth-scale readability

The asset readme must document generation source, prompt/source description,
where each asset is used, and that concept art files were references only.

## Scene Plate And Prompt Rules

The runtime scene plate should use written scene descriptions only. Historical
maps may not be used as runtime image references for this pass.

Prompt/metadata language should prefer:

> wide elevated oblique illustrated storybook game scene

Do not require strict isometric or isomorphic projection. The renderer handles
player/NPC/exit placement with anchors and depth scaling instead.

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
4. player/NPC markers with depth sorting and scale
5. selection ring/callout
6. top parchment ribbon
7. left Nearby portrait strip
8. right notebook page/tabs
9. bottom action stamps
10. bottom intent command strip
11. Map/Time cards
12. Active Intents card

Svelte should host the canvas, subscribe to stores, pass render props into the
renderer, submit commands, and open secondary overlays only when requested.

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
- Unit tests prove marker sorting/scaling and command input submit behavior.
- `parish-world` tests prove prompt/metadata language rejects historical map
  dependencies and strict isometric/isomorphic requirements.
- Desktop and mobile screenshots prove the first viewport meets the notebook
  concept and lacks the old dashboard/InputField treatment.
