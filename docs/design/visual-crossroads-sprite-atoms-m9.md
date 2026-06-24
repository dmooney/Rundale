# visual-crossroads-sprite-atoms-m9

This milestone turns the visually promising Crossroads scene into a genuine
sprite-compositor proof. The goal is to preserve the current screenshot's
coherent pixel-art look while making the implementation depend on local,
reusable transparent atoms instead of full-frame building/wall/sign slices.

## Affected Subsystems

- `mods/rundale/scenes.json`: Crossroads assets/layers become local atoms with
  explicit x/y/anchor/scale/z.
- `mods/rundale/assets/scenes/the-crossroads/atoms/`: add or replace local PNG
  atom assets for church/rise, pub, walls, signpost, brambles, road/wetness
  accents.
- `parish/apps/visual/src/pixi-renderer.js`: should already support local atom
  rendering; adjust only if full-stage assumptions remain.
- `parish/apps/visual/src/renderer.test.mjs` or regression tests: assert
  Crossroads has meaningful local atom placement.

## Data Model

No schema change expected. This uses existing `SceneState.layers` fields:
`asset_url`, `x`, `y`, `z`, `scale`, `anchor`, `opacity`, and layer `kind`.

## Observable Signal

The `/scene` transcript should prove the data shape by listing local Crossroads
layers at non-center coordinates. Screenshots prove the visual bar: the
Crossroads must still look like the loved pixel-art render.

## Feature Flag

No new flag. This is asset/data work on the existing graphical client.
