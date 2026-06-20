# Visual Client M3

This milestone makes the separate graphics client character-aware. The player
sees backend-authored NPC sprites over the scene plate and can click a sprite to
prepare a conversational command. The visual app remains a small browser client
using the existing Parish server contract; Tauri/Svelte renderer work stays out
of scope.

## Affected Subsystems

- `parish/apps/visual`: loads sprite images, caches them with plate images,
  draws them at stage-space foot anchors, tracks hover/selection across hotspots
  and NPCs, and maps sprite clicks to command-input text.
- `parish/crates/parish-server`: consumed as-is through `/api/scene-state`,
  `/api/scene-asset/*`, and `/api/command`.
- `parish/apps/ui`: intentionally untouched for this milestone.

## Data Model

No backend data model changes are required. The visual display model extends
scene NPCs with sprite hit bounds derived from:

- `x` and `y` as the foot-anchor in the 1280x720 stage coordinate system;
- `scale` from scene-state;
- a 48x72 authored sprite baseline, matching the current fallback sprite asset.

The renderer accepts a `spriteImages` map keyed by NPC id. Missing or failed
sprites fall back to a compact marker so image failures remain visible without
blocking the rest of the scene.

## Observable Signal

The harness signal is Darcy's Pub `/scene`: populated NPC slots include
`sprite: assets/scenes/sprites/generic-villager.png`. The browser signal is the
visual client showing the sprites on the pub plate, then changing the command
input after a sprite click to a `talk to ...` command.

## Feature Flag

No new flag. Backend scene-state remains gated by the existing `diorama` flag.
