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
- Avoid committing generated-image cache paths. Copy selected renders into this
  folder and reference them relatively from Markdown.
