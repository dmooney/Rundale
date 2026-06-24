# Visual Game Slice M1 Design Note

> Status: Approved for implementation. Task: `visual-game-slice-m1`.

## Player Experience

Rundale's visual client becomes a game-first browser experience. On launch the
player sees a full-screen illustrated Kilteevan Village scene, with clickable
places and visible NPCs placed inside the world. Movement to The Crossroads and
Darcy's Pub happens through scene clicks with a short transition, while text is
kept to a compact caption/log and command fallback at the bottom of the screen.

## Affected Subsystems

- `parish-mod`: scene schema validation for duplicate authoring ids.
- `parish-core`: additive scene-state hotspot activation hints and `/scene`
  proof text.
- `parish-server` and `parish-tauri`: no behavioral fork; they continue mapping
  the shared scene state to HTTP URLs or data URLs.
- `parish/apps/visual`: full-screen PixiJS renderer, input handling, caption
  log, command fallback, and responsive layout.
- `mods/rundale`: compositor atoms for the three-scene slice, with a distinct
  Kilteevan Village scene.

## Data Model

`SceneState` remains schema version 1 and keeps legacy fields. Hotspot views gain
a deterministic activation hint so visual clients can submit a command without
inferring it from display text. The first supported hint is travel command text
resolved from the target world location name; inspect hotspots carry their
existing authored text. The scene file format remains additive and continues to
use the existing `diorama` feature flag.

## Observable Signals

The script fixture proves the shared backend contract by printing `/scene` for
Kilteevan Village, The Crossroads, and Darcy's Pub. Browser proof must include
desktop and mobile screenshots showing the first viewport as a graphical game,
plus interaction evidence for a clicked exit, an inspected hotspot, and an NPC
selection.
