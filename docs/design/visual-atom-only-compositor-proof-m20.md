# Visual Atom-Only Compositor Proof M20

## Player Experience

Players should keep seeing the same full-screen pixel-art adventure client. This milestone adds invisible proof telemetry so we can demonstrate that the scene is built from ordered PNG atoms in Pixi, not from a hidden full-frame legacy plate. The explicit `visualProofMode=atom-only` query parameter may enable stricter atom-only behavior for automation, but it must not show labels, outlines, debug panels, or any new first-read UI.

## Affected Subsystems

- `parish/apps/visual`: Pixi renderer telemetry, proof-only strict mode, regression tests, and live browser proof.
- `parish/testing/fixtures`: deterministic `/scene` script for the three-scene slice.
- `mods/rundale`: no content changes expected.
- Backend crates: no expected behavior change. Existing `/scene` state remains authoritative.

## Data Model

No backend schema changes. The client adds runtime-only telemetry:

- `window.__rundaleVisualCompositor.mode`
- `window.__rundaleVisualCompositor.slug`
- `window.__rundaleVisualCompositor.layerSprites`
- `window.__rundaleVisualCompositor.npcSprites`
- `window.__rundaleVisualCompositor.hotspotCues`
- `window.__rundaleVisualCompositor.fallbackPlateUsed`

In proof atom mode, the Pixi renderer must not draw the legacy `model.underlay` or `model.plate` fallback. If the scene has no layers, it should fail visibly as an empty/failed render in telemetry rather than quietly hiding the problem behind a full plate.

## Observable Signals

- Static tests verify the default path still has no visible debug overlay and that proof mode is query-string driven.
- Renderer tests or regression tests verify the Pixi source records layer sprite telemetry and distinguishes layer sprites from fallback plate sprites.
- Live browser proof visits Kilteevan Village, The Crossroads, and Darcy's Pub with proof atom mode enabled, captures screenshots, and records telemetry showing:
  - layer sprite counts above each scene's threshold,
  - `fallbackPlateUsed: false`,
  - expected landmark layer IDs are present,
  - screenshots are nonblank.

## Feature Flag

No engine feature flag. This is a visual-client proof mode controlled only by the `visualProofMode=atom-only` URL query parameter for browser automation.
