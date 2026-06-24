# Visual Raster Hotspot Cues M17 Plan

1. `feat:` Add small transparent PNG cue assets under `parish/apps/visual/assets/cues/`.
2. `feat:` Load cue textures during `PixiSceneRenderer.init()` and render active/selected hotspot feedback as `PIXI.Sprite` instances positioned from hotspot bounds.
3. `test:` Update visual regression tests to assert hotspot cues use raster sprite assets and do not contain the old code-drawn helper path.
4. `proof:` Run visual checks, atom audit, backend movement transcript, and a live browser proof with desktop/mobile screenshots.
