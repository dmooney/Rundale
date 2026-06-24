# Visual Compositor QA Baseline M21

## Player Experience

Players should not see a new UI feature. The experience remains a fullscreen pixel-art adventure scene composed from layered PNG atoms. This milestone adds stricter behind-the-scenes QA so future art changes cannot quietly drift back into full-frame renders, SVG placeholders, blank transparent crops, or mis-scaled shadow/lighting sheets that create visible rectangular seams.

## Affected Subsystems

- `parish/apps/visual`: atom audit script, audit tests, and proof automation.
- `mods/rundale/assets/scenes`: existing PNG atom stacks are measured but not expected to change.
- `parish/testing/fixtures`: deterministic movement fixture for the three-scene visual slice.
- Backend crates: no schema or route changes expected.

## Data Model

No backend data-model change. The audit enriches local QA output with derived image metrics:

- visible pixel count and coverage per atom,
- scene-level meaningful atom count,
- blank or near-blank atom list,
- suspicious full-stage atom list,
- full-stage effect overlays that must match `SceneState.native_size`.

These metrics are build/proof artifacts only. They do not enter `SceneState` and must not create visible debug affordances in the game client.

## Observable Signals

- `npm --prefix parish/apps/visual run audit:atoms` prints per-scene contribution metrics and exits nonzero for any blank atom, missing/SVG asset, suspicious full-frame atom, or mis-sized full-stage effect overlay.
- Visual-client tests cover the negative cases with controlled fixtures or static assertions.
- Live browser proof runs in `visualProofMode=atom-only`, records Pixi telemetry for Kilteevan Village, The Crossroads, and Darcy's Pub, and captures nonblank screenshots without a visible debug overlay.
- The script fixture travels through Kilteevan Village, The Crossroads, and Darcy's Pub and emits `/scene` state at each stop.

## Feature Flag

No engine feature flag. This is a QA/proof milestone for the visual client and atom asset audit. Browser automation continues to use the existing `visualProofMode=atom-only` URL query parameter.
