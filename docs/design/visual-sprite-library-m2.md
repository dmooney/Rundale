# Visual Sprite Library M2 Design Note

> Status: Implemented on `codex/visual-sprite-library-m2`.
> Task: `visual-sprite-library-m2`.

## Player Experience

Kilteevan Village keeps the appealing pixel-art mood of the current graphical
slice, but the camera and construction become more game-like: a high 3/4
isometric/oblique village assembled from reusable transparent PNG atoms. The
scene should show roofs, ground footprints, foot anchors, and readable depth
sorting, rather than the lower Crossroads-style camera.

## Affected Subsystems

- `mods/rundale`: expands Kilteevan's reusable atom kit and recomposes the
  scene from repeated PNG layer instances.
- `parish-mod` / `parish-core`: keep the existing additive scene contract, while
  tests assert the layer stack remains deterministic.
- `parish/apps/visual`: renders the same ordered Pixi layer stack, with added
  proof coverage for repeated atoms and non-plate composition.

## Data Model

No breaking scene schema change is planned. Existing `SceneAsset` and
`SceneLayer` fields carry the atom URL, kind, position, scale, opacity, labels,
and y-depth order. If needed, this milestone may add non-runtime audit metadata
only; the renderer continues consuming ordinary scene JSON.

## Observable Signals

The fixture prints `/scene` for Kilteevan, The Crossroads, and Darcy's Pub.
Kilteevan proof must show repeated `atoms/kit/` PNG assets, no SVG layer URLs,
valid activation hints, preserved NPC slot data, and screenshots where the
first viewport reads as a high 3/4 playable adventure scene.

## Implementation Status

M2 adds 25 generated-and-cleaned transparent PNG atoms under
`mods/rundale/assets/scenes/kilteevan-village/atoms/kit/` and places 41 new
`m2-` layer instances into Kilteevan Village. The set covers road, mud, puddles,
wall, foliage, cottage detail, prop, smoke, sign, and wood families. Four M2
assets are deliberately reused in at least three distinct positions, and the
visual atom audit now requires Kilteevan to keep four reusable families
(`water`, `wall`, `foliage`, and `terrain_patch`) plus at least 32 kit layers.

The live proof captured desktop and mobile first viewports, a well inspection,
NPC selection, and click travel through Kilteevan -> The Crossroads -> Darcy's
Pub. Browser compositor telemetry confirmed `fallbackPlateUsed=false` and zero
missing layer assets during the captures.

This does not finish the full Factorio-style authoring pipeline. Kilteevan
still uses larger base/local atoms below the new kit. The next slice should
continue breaking those terrain and building chunks into reusable families while
preserving the high 3/4 camera and the current pixel-art quality bar.
