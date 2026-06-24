# Design: Visual Named NPC Sprites M5

The player should see actual characters in the world, not the same generic
person repeated everywhere. This pass adds project-local raster PNG sprite
assets for named NPCs that appear in the playable slice, then maps those assets
through the existing `sprites` section in `mods/rundale/scenes.json` so
`SceneState.npcs[*].sprite_url` becomes character-specific.

Affected subsystems:

- `mods/rundale/assets/scenes/sprites/`: add character PNG sprites.
- `mods/rundale/scenes.json`: add `sprites` entries for NPC ids `1`, `8`, and
  `22`; keep fallback sprites for everyone else.
- `parish-mod`: assert the real Rundale scene index has the named sprite
  definitions and validates their files.
- `parish-core`: existing `scene_npc_view` already prefers `sprite_for(npc.id)`
  before fallback; tests can assert the behavior remains true.
- `parish-server`: assert `/api/scene-state` exposes cache-busted named sprite
  URLs for the visible NPCs.
- `parish/apps/visual`: no renderer contract change expected; Pixi already
  loads `npc.spriteUrl` and falls back only when missing.

Data model:

- No schema change. Use existing `SpriteDef { npc_id, image }`.
- Add three named PNG sprite assets and three `sprites` entries.

Observable signal:

- `parish-engine --script parish/testing/fixtures/play_visual-named-npc-sprites-m5.txt`
  prints `/scene` lines showing `peig-hannigan.png`, `padraig-darcy.png`, and
  when present `niamh-darcy.png` instead of the fallback sprite path.
- Live browser proof clicks into Darcy's Pub and selects Padraig.

Feature flag:

- None. This is an authored visual content improvement over an existing
  default-on scene contract.
