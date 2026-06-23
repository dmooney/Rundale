# Graphic Compositor M1 Design Note

> Status: Review draft · Task: `graphic-compositor-m1`

## Player Experience

When graphical mode is enabled, Rundale begins to show location scenes composed
from semantic scene data rather than only opaque background plates. The first
milestone keeps the current plate-based bridge alive, but adds compositor
structure so the player can start at Kilteevan Village, click or inspect
hotspots, and move through the current visual slice without existing clients
breaking.

## Affected Subsystems

- `parish-mod`: scene schema, asset validation, cross-reference warnings, load
  summaries.
- `parish-core`: shared scene-state builder, `/scene` text renderer, IPC types.
- `parish-server`: `/api/scene-state` and `/api/scene-asset/{*rel}` continue to
  map mod assets safely.
- `parish-tauri`: `get_scene_state` continues to map scene assets to frontend
  data URLs.
- `parish/apps/ui`: `DioramaView` remains compatible with legacy scene fields
  during the migration.
- `parish/apps/visual`: Canvas client remains compatible with legacy scene
  fields and can ignore new compositor fields until a later milestone.
- `mods/rundale`: `scenes.json` becomes the first compositor data source.

## Data Model

`SceneIndex` remains optional and keeps existing fields (`plate`, `variants`,
`hotspots`, `slots`, `sprites`, `fallback_sprites`). M1 adds compositor data
additively:

- `native_size`: authored scene dimensions.
- `assets`: reusable visual atoms with ids, images, anchors, and kinds.
- `layers`: scene instances with asset id, coordinates, z order, scale,
  opacity/flip as needed, and optional runtime labels.
- `underlay`: optional image reference, used only as a transitional aid.
- `labels`: runtime text overlays for sign-like assets, validated against text
  budget and known destinations where applicable.

The existing `diorama` feature flag remains the gate for runtime exposure; no
new feature flag is introduced in this milestone.

## Observable Signal

The verification fixture proves the feature through `/scene`: flag-off returns
no scene, flag-on returns active scene data at Kilteevan Village, The
Crossroads, and Darcy's Pub, and the output includes both legacy fields and
compositor layer data.
