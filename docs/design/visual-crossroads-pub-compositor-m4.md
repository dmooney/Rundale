# Design: Visual Crossroads/Pub Compositor M4

The player experience should no longer reveal that only Kilteevan is a layered
game scene while the rest of the playable slice is a static plate. The
Crossroads and Darcy's Pub should still preserve their current composition and
hotspots, but their `SceneState.layers` should describe a sprite-composited
stack of raster PNG atoms: a muted base plus landmark, prop, light, shadow, and
foreground effect layers.

Affected subsystems:

- `mods/rundale/scenes.json`: add raster asset declarations and replace the
  single `pixel-plate` layers for The Crossroads and Darcy's Pub with ordered
  named atom layers.
- `mods/rundale/assets/scenes/**/atoms/`: add PNG layer assets for each scene.
- `parish-mod`: harden real-content tests so the three-scene slice stays
  compositor-driven.
- `parish-server`: assert `/api/scene-state` exposes the new layers with
  cache-busted asset URLs.
- `parish/apps/visual`: no renderer contract change is expected; this pass
  exercises existing Pixi layer ordering, stage scaling, hotspot, and NPC logic.

Data model:

- No schema change. Existing asset `kind`, layer `z`, opacity, scale, and
  legacy `underlay`/`plate` fields remain compatible.
- Crossroads and Pub keep their legacy `underlay`/`plate` fields as fallback
  metadata, but live rendering should come from ordered `layers`.

Observable signal:

- `parish-engine --script parish/testing/fixtures/play_visual-crossroads-pub-compositor-m4.txt`
  prints `/scene` lines where The Crossroads and Darcy's Pub each show several
  named PNG atom layers instead of one `pixel-plate`.
- Live browser proof clicks the same three-scene route and captures desktop and
  mobile screenshots.

Feature flag:

- None. This is content/visual-client milestone work on an already default-on
  additive scene contract, not a new gameplay rule.
