# Plan: Illustrated Notebook Real Play Screen

Status: Pre-implementation plan

## Commit 1: `feat(ui): add pixi notebook play surface`

1. Add PixiJS to `parish/apps/ui/package.json` and refresh the package lock
   without unrelated dependency churn.
2. Create `src/components/illustrated-notebook/IllustratedNotebookGame.svelte`
   as the Svelte host for a full-viewport Pixi canvas plus hidden/minimal
   accessibility input.
3. Create `src/lib/illustrated-notebook/renderer.ts` with explicit Pixi layer
   containers matching the acceptance criteria.
4. Create `src/lib/illustrated-notebook/layout.ts` with desktop/mobile layout
   math, depth anchor mapping, and deterministic marker scaling.
5. Switch `routes/+page.svelte` to mount `IllustratedNotebookGame` as the
   default play viewport while retaining setup/save/map/debug/demo/bug/mod
   overlays as secondary surfaces.

## Commit 2: `feat(ui): add generated notebook asset kit`

1. Create `parish/apps/ui/static/rundale/notebook-ui/`.
2. Generate or create original bitmap assets for the top ribbon, spiral
   notebook, binding/rings, tabs, bottom intent strip, input line, send stamp,
   action stamps/icons, portrait frames/placeholders, Active Intents card, Map
   card, Time card, exit label, selection ring, player marker, and NPC markers.
3. Add an asset manifest consumed by the Pixi renderer.
4. Add `asset-readme.md` documenting prompts/source descriptions, usage, and
   that the concept image was not sliced.
5. Keep any old `static/notebook-ui` experiment available only if unused by the
   first viewport or explicitly deleted in a separate cleanup.

## Commit 3: `feat(world): add notebook visual scene metadata`

1. Add additive visual-scene metadata for the default Rundale/Kilteevan scene:
   written visual summary, plate asset path, camera hint using "wide elevated
   oblique illustrated storybook game scene", scene anchors, and depth bands.
2. Verify the runtime plate provenance is written-description-only. If the
   existing plate cannot be proven clean, generate a new plate and document its
   prompt.
3. Add `parish-world` tests that reject historical-map-reference language and
   strict isometric/isomorphic requirement language in runtime visual-scene
   metadata.
4. Keep "isometric" references out of new runtime prompts except in tests or
   docs that explicitly identify old rejected experiments.

## Commit 4: `feat(ui): render notebook gameplay layers in pixi`

1. Render the world background plate as the root Pixi scene layer.
2. Render exit labels, player marker, nearby NPC markers, selection ring, and
   callout with depth sorting and scale bands.
3. Render the top parchment ribbon with title, location, time, weather, and
   compass.
4. Render the left Nearby portrait strip from `npcsHere`.
5. Render the right spiral notebook page/tabs for the selected/default NPC,
   including sketch/portrait, name, mood, occupation, trust dots, known facts or
   placeholder, and witness information when present.
6. Render bottom action stamps, Map/Time cards, Active Intents card, and bottom
   intent strip.
7. Add hover/focus/selected/busy states in Pixi without falling back to DOM
   boxes in the first viewport.

## Commit 5: `feat(ui): replace visible command input`

1. Build `NotebookCommandInput` controller/helpers for visible Pixi text,
   placeholder, caret/focus, disabled/busy state, and send affordance.
2. Use a hidden/minimally styled native input or textarea only for keyboard,
   accessibility, clipboard, and IME.
3. Submit through existing `submitInput`, support Enter submit, preserve error
   routing, and clear/update state after submit.
4. Make action stamps seed intent text and focus the hidden input.
5. Add focused tests for submit, Enter handling, busy disabled behavior, action
   seeding, and old `InputField.svelte` absence from the default viewport.

## Commit 6: `feat(ui): notebook secondary overlays`

1. Wire right-side tabs/cards to open Journal/chat, People, Focail, Map,
   Save/Load, Debug, Mod, and Bug Report as overlays/drawers.
2. Reuse existing Svelte surfaces inside those secondary overlays only where
   appropriate.
3. Ensure none of those surfaces render persistently in the default first
   viewport.

## Commit 7: `test(ui): verify responsive notebook presentation`

1. Add focused unit/component tests for marker scaling/depth sorting and command
   input behavior.
2. Run `fnm exec --using 22 npm run check`.
3. Run `fnm exec --using 22 npm run lint`.
4. Run `fnm exec --using 22 npm run format:check`.
5. Run `fnm exec --using 22 npm run build`.
6. Run the relevant `parish-world` visual-scene tests.
7. Run the backend fixture:
   `cd parish && cargo run -p parish-engine -- --script
   testing/fixtures/play_illustrated-notebook-real.txt`.
8. Start the built app against Rundale, capture:
   `.proofs/illustrated-notebook-real/desktop.png` at 1440x900 and
   `.proofs/illustrated-notebook-real/mobile.png` at 390x844.
9. Open both screenshots and compare against
   `docs/graphics-v2/illustrated-parish-notebook.png`.
10. Write `.proofs/illustrated-notebook-real/evidence.md` and `judge.md` with
    explicit visual-pass notes.
11. Run `just agent-check`.

## Rework Triggers

- The first viewport still reads as old UI over a background.
- `InputField.svelte`, `.input-wrapper`, `.input-form`, a rectangular text box,
  or an old `Send` button is visible in the first viewport.
- Persistent `StatusBar`, `ChatPanel`, `Sidebar`, map panel, mobile toolbar, or
  debug/dev toolbar is visible in the first viewport.
- Most notebook UI pieces are CSS rectangles/SVG placeholders rather than
  original generated bitmap assets.
- The runtime background/provenance depends on historical map image references.
- The prompt/metadata insists on strict isometric/isomorphic projection.
- Desktop or mobile screenshots are not captured from the running app.

Any rework trigger is a requirement failure, not future polish.
