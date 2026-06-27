# Graphics V2 Folder Notes

This folder contains exploratory concept art and prompt artifacts for a possible
graphical version of Rundale. These files are design references, not production
runtime assets.

## What Is Here

- `concept-7a-conversation-lens.png` — original reference for selected-NPC
  conversation gameplay. Useful for understanding the interaction model:
  select a person, read mood/knowledge, choose a compact action or type intent.
- `concept-7c-roads-and-schedules.png` — original reference for the preferred
  wider isometric zoom level. Useful for camera distance, exits, road context,
  and schedule/navigation readability.
- `illustrated-parish-notebook.png` — selected visual direction combining the
  7A conversation gameplay with the wider 7C camera.
- `illustrated-parish-notebook-prompt.md` — the prompt that produced the
  selected notebook UI render.
- `illustrated-parish-notebook-standalone-prompt.md` — a self-contained prompt
  for one-shot testing in other image models, replacing references to 7A/7C
  with explicit text.
- `illustrated-parish-scene-no-ui.png` — environment-only plate in the same
  illustrated style, with no UI.
- `illustrated-parish-scene-no-ui-prompt.md` — layout-first prompt for the
  no-UI environment plate, with explicit river, bridge, road, building, field,
  and footpath continuity constraints.
- `portable-background-plate-one-shot-template.md` — generic cleanroom prompt
  template for historic-map-tile-to-background-plate experiments.
- `one-shot-background-plate-test-protocol.md` — how to test one-shot prompts
  without contaminating the model context with previous failed/generated plates.
- `map-crop-selection-protocol.md` — how to choose/back-calculate historic map
  crops from desired plate scale instead of letting arbitrary source-map zoom
  drive the output composition.
- `map-to-background-plate-pipeline.md` — proposed reproducible map-reader and
  control-plate pipeline for consistent scale/perspective without hand-authored
  per-location hints.
- `map-reader-stage-template.md` — current generic rubric for producing
  reproducible, confidence-graded map-reader notes from a map crop before image
  generation.
- `map-to-background-plate-research-plan.md` — broader brainstorm and experiment
  plan for raw-map, warped-map, semantic-mask, blockout, procedural-render, and
  segmentation-model pipeline variants.
- `scripts/prototype_map_controls.py` — dependency-free prototype that emits
  rough control images from a historic map crop for clean-context experiments.
- `pipeline-experiments/` — generated control images and pipeline experiment
  renders. Treat these as research artifacts, not runtime assets.
- `one-shot-background-plate-candidate-cycle-j.md` — current best generic
  prompt/reference set for cleanroom historic-map-to-background-plate tests on
  Grove and Beechwood.
- `one-shot-background-plate-candidate-cycle-k.md` — current best candidate when
  building interpretation matters; uses the reproducible map-reader stage plus
  the Cycle J cleaned style swatches.
- `two-step-topdown-isomorphic-cycle-l.md` — experiment testing a top-down
  cleaned control plate followed by an isomorphic conversion; promising for
  stronger source topology, with native 16:9 framing still needing work.
- `grove-cleanroom-test-notes.md` — Grove experiment log and map-reading case
  study. Do not treat it as a reusable prompt; keep the reusable prompt generic.
- `style-crops/` — manually cropped style references from approved illustrated
  concepts. Check crops visually before using them; reject crops with labels,
  UI marks, signs, or strong whole-scene composition signals.

## Working Guidelines

- Preserve the original images unless the user explicitly asks to replace them.
  Add new variants with descriptive filenames.
- Save the prompt beside any generated image so the idea can be reproduced or
  tested in another model.
- For environment plates, write layout constraints before style notes. Be
  explicit about rivers, bridges, roads, paths, and building placement; generated
  scenes often look beautiful while containing impossible geography.
- Keep prompts self-contained when they are intended for cross-model testing.
  Do not rely on hidden context or previous chat images unless the file clearly
  says it is reference-dependent.
- For one-shot tests, prefer fresh subagents or fresh model sessions with only
  the target map crop plus the intended reference images. Do not evaluate a
  prompt using a context that has already seen failed renders.
- Do not create location-specific reusable prompts or hand-authored per-location
  hint notes. It is acceptable to create location-specific map-reader notes only
  when they are generated from the map crop by the same reproducible
  clean-context rubric for every location, saved as auditable artifacts, and
  kept confidence-graded.
- Treat historic map crop size as arbitrary source context, not as the output
  composition. Pick the desired plate scale, sprite scale, and orthographic
  camera first; then use or back-calculate the map crop needed to cover the
  named site's local area and exits.
- Keep reusable background-plate prompts north-up unless the user explicitly
  asks otherwise: source-map top should remain final-image top, with the 3/4
  game feel coming from oblique pitch and building extrusion rather than
  rotating the ground plan.
- Treat ambiguous historic-map linework conservatively in the generic prompt.
  Single thin lines are usually plot boundaries, hedges, walls, ditches, or
  overgrown walls, not extra footpaths. If more precision is needed, run the
  reproducible map-reader stage or another repeatable control process rather
  than writing hand-authored per-location notes.
- Treat base plates as static art layers. Avoid visible smoke/fog/weather in
  the plate; those belong in later runtime/composited layers.
- Avoid committing generated-image cache paths. Copy selected renders into this
  folder and reference them relatively from Markdown.
- For production-style one-shot tests, prefer small style/material swatches over
  full illustrated scenes. Full scenes are visually attractive but can leak
  landmarks, bridges, UI, props, and whole-scene layouts into unrelated map
  crops.
- Prefer Cycle K for building-heavy plates: run the reproducible map-reader
  stage first, then pass the map crop, map-reader note, and cleaned style
  swatches into the render prompt. The map remains the source of truth; the note
  is soft disambiguation with confidence.
- Use Cycle L-style top-down cleaned plates when topology is hard to read. They
  are promising as an accuracy path, but do not treat them as source truth; the
  original map crop and map-reader note still outrank the cleaned plate. Before
  making Cycle L the default final-render path, solve native 16:9 framing
  without edge-extension artifacts.
