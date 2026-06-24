# Visual Ambient Layer Animation M16

This slice makes the existing raster scene compositor feel less static by letting individual PNG layers carry optional ambient animation metadata. A player should still see the same full-screen pixel-art adventure scenes, but smoke can drift, wet paths can shimmer, and pub lights can flicker as live compositor sprites rather than as CSS effects or a new baked background.

Affected subsystems:

- `parish-mod`: adds additive scene JSON schema for optional per-layer animation and validates animation bounds.
- `parish-core`: carries animation metadata through `SceneState.layers` and the `/scene` text renderer.
- `parish/apps/ui`: keeps TypeScript scene-state types aligned with the backend contract.
- `parish/apps/visual`: maps animation metadata into the display model and updates Pixi sprites on the ticker.
- `mods/rundale`: annotates selected Kilteevan, Crossroads, and Darcy's Pub PNG atom layers.

Data model:

- `SceneLayer.animation?: SceneLayerAnimation`
- `SceneLayerAnimation.mode`: `drift`, `shimmer`, or `flicker`
- `amplitude_x`, `amplitude_y`, and `alpha` describe small sprite-relative motion and opacity changes.
- `period_ms` and `phase` make the motion deterministic and vary repeated atoms.

Observable signal:

- The proof fixture prints `/scene` at all three slice locations. Animated layers include `animation: <mode>` in the text output.
- Browser proof captures two frames over time and records nonzero canvas pixel delta for the animated scene.

Feature flag:

- None. This is additive presentation metadata; layers without `animation` keep current behavior, and non-visual frontends can ignore the optional field.
