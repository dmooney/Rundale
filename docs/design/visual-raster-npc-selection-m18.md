# Visual Raster NPC Selection M18

## Player Experience

When the player moves the pointer over an NPC, the character should read as a clickable person in the world. The cue should look like a small pixel-art asset layered under the sprite, not a debug ellipse drawn by Pixi. Clicking the NPC keeps the cue selected, shows the compact bottom `Talk` prompt, and prepares the command fallback without starting a conversation until the player confirms.

## Affected Subsystems

- `parish/apps/visual`: Pixi renderer cue loading/drawing, static regression tests, proof browser automation.
- `parish/testing/fixtures`: deterministic script proving the NPC scene-state slots remain available in Kilteevan Village and Darcy's Pub.
- `mods/rundale`: no scene schema changes expected; this milestone consumes the existing NPC sprite/slot data.
- Backend crates: no expected behavior change. Existing `/scene` contract and `SceneState.npcs` remain unchanged.

## Data Model

No backend schema changes. The milestone adds a local client raster cue asset:

- `parish/apps/visual/assets/cues/npc-select.png`

`PixiSceneRenderer` should preload the texture once and reuse it for hover/selection redraws, keeping pointer movement synchronous.

## Observable Signals

- Static test sees `npc-select.png`, sees Pixi sprite-based NPC cue rendering, and rejects the old `PIXI.Graphics().ellipse(...)` NPC highlight path.
- Live browser proof captures:
  - Kilteevan Village NPC hover/select with `Talk <scene-state display label>`.
  - Darcy's Pub NPC hover/select with `Talk <scene-state display label>`, preferring the named pub slots when they are occupied.
  - Command input populated with `talk to ...` while no dialogue is auto-sent.
- Script transcript shows `/scene` slots and sprite URLs for the NPCs in those locations.

## Feature Flag

No new engine feature flag. This is a visual-client renderer milestone using already-shipped scene state.
