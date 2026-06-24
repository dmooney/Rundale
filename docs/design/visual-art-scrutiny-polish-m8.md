# visual-art-scrutiny-polish-m8

This pass treats the current screenshots as game-art review material rather
than as a mere functional proof. The player experience should still be
immediate point-and-click play, but hover feedback and camera framing should
feel authored for a pixel-art adventure game, not exposed tooling.

## Affected Subsystems

- `parish/apps/visual/src/pixi-renderer.js`: adjust hotspot hover rendering and
  mobile scene framing.
- `mods/rundale/scenes.json`: tune Darcy's Pub NPC slots so named sprites read
  as intentionally placed.
- `parish/apps/visual/src/main.js`: no expected behavior change; M8 should
  preserve M7 action prompts.
- `parish/apps/visual/src/main-regression.test.mjs` and renderer tests: add
  small source/model checks for the new polish contract where practical.

## Data Model

No schema change. This is renderer behavior plus authored scene slot tuning.

## Observable Signal

The harness signal remains the three-scene `/scene` transcript. The main M8
acceptance signal is live screenshots: hover feedback should no longer look like
a large debug rectangle, mobile first-read should show richer village context,
and pub NPCs should read as intentionally positioned.

## Feature Flag

No new flag. This is visual-client polish on top of the existing graphical
client path.
