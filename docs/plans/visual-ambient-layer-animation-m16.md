# Visual Ambient Layer Animation M16 Plan

1. `feat:` Add optional scene-layer animation schema in `parish-mod`, including validation for mode, period, amplitudes, alpha, and phase.
2. `feat:` Propagate animation metadata through `parish-core::ipc::scene::SceneLayerView`, `/scene` text, and frontend TypeScript scene-state types.
3. `feat:` Map animation metadata in the visual display model and update `PixiSceneRenderer` so animated PNG layer sprites drift, shimmer, or flicker on the Pixi ticker.
4. `feat:` Annotate Kilteevan smoke, Crossroads wet/water glints, and Darcy's Pub warm-light atoms in `mods/rundale/scenes.json`.
5. `test:` Extend Rust scene-state/schema tests and visual renderer tests; run the existing atom audit to make sure the compositor still uses PNG atoms.
6. `proof:` Run backend checks, visual `check`/`test`/`build`, live browser proof with desktop/mobile screenshots, write evidence and judge, then run local `agent-check`.
