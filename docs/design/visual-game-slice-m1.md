# Visual Game Slice M1 Design Note

> Status: Approved for implementation. Task: `visual-game-slice-m1`.

## Status Report - 2026-06-24

Visual Game Slice M1 is implemented and verified on branch
`codex/visual-game-slice-m1`. It is ready to review against the `graphic`
integration branch.

What landed:

- Full-screen visual client in `parish/apps/visual`, with PixiJS as the primary
  renderer and a canvas fallback for proof and resilience.
- Additive scene-state contract for ordered layers, native size, hotspot
  activation hints, slots, NPC sprites, labels, and layer animation metadata.
- Three-scene playable slice: Kilteevan Village, The Crossroads, and Darcy's
  Pub.
- Raster PNG scene atoms, prop-kit atoms, named NPC sprites, and raster
  hover/selection cues. The milestone does not target SVG placeholders.
- Deterministic scene validation for duplicate location ids/slugs, hotspot ids,
  slot ids, and NPC sprite ids.
- Live proof artifacts under `.proofs/visual-game-slice-m1` and follow-up visual
  proof bundles through M22.

Validation run before PR:

- `npm --prefix parish/apps/visual run check`
- `npm --prefix parish/apps/visual run test`
- `npm --prefix parish/apps/visual run build`
- `npm --prefix parish/apps/visual run audit:atoms`
- `just verify` with Node 22, `RUSTC_WRAPPER=`, and `CARGO_TARGET_DIR=target`
- Focused 20-run stress check for the fixed
  `guard_does_not_fire_on_grounded_person_name_topic_continuity` flake.

Current assessment:

This is a good visual proof and a credible direction. It reads as a graphical
adventure-game client rather than a dashboard, and the compositor path is real.
The most important remaining gap is that the asset pipeline is not yet a
production-quality sprite library.

Known issues and next risks:

- The compositor is real, but many atoms are still large local scene chunks.
  This proves layered PNG composition, not yet a Factorio-like library of small
  reusable terrain, wall, road, foliage, prop, shadow, and decal sprites.
- The art direction is close but not locked. Some PNGs still show AI softness,
  fuzzy alpha, uneven pixel discipline, and slight perspective or lighting
  mismatch between props.
- Asset weight is high for an M1. The next pass needs compression, atlasing,
  lazy loading, and explicit per-scene asset budgets.
- Authoring is too manual. `scenes.json` now has enough expressive power to be
  useful, but layer placement and sizing need an editor or at least a placement
  tool that exports clean scene data.
- Gameplay verbs are still thin. Movement, inspect, and NPC selection are
  present; richer hotspot responses, first dialogue beats, and more consequential
  click actions should follow.
- Mobile was exercised, but it needs more polish for touch targets, text density,
  loading behavior, and whether the first viewport still reads as a game world.
- Visual regression protection is early. The atom audit is useful, but CI should
  eventually include Playwright screenshot or canvas checks for desktop and
  mobile viewports.

## Player Experience

Rundale's visual client becomes a game-first browser experience. On launch the
player sees a full-screen illustrated Kilteevan Village scene, with clickable
places and visible NPCs placed inside the world. Movement to The Crossroads and
Darcy's Pub happens through scene clicks with a short transition, while text is
kept to a compact caption/log and command fallback at the bottom of the screen.

## Affected Subsystems

- `parish-mod`: scene schema validation for duplicate authoring ids.
- `parish-core`: additive scene-state hotspot activation hints and `/scene`
  proof text.
- `parish-server` and `parish-tauri`: no behavioral fork; they continue mapping
  the shared scene state to HTTP URLs or data URLs.
- `parish/apps/visual`: full-screen PixiJS renderer, input handling, caption
  log, command fallback, and responsive layout.
- `mods/rundale`: compositor atoms for the three-scene slice, with a distinct
  Kilteevan Village scene.

## Data Model

`SceneState` remains schema version 1 and keeps legacy fields. Hotspot views gain
a deterministic activation hint so visual clients can submit a command without
inferring it from display text. The first supported hint is travel command text
resolved from the target world location name; inspect hotspots carry their
existing authored text. The scene file format remains additive and continues to
use the existing `diorama` feature flag.

## Observable Signals

The script fixture proves the shared backend contract by printing `/scene` for
Kilteevan Village, The Crossroads, and Darcy's Pub. Browser proof must include
desktop and mobile screenshots showing the first viewport as a graphical game,
plus interaction evidence for a clicked exit, an inspected hotspot, and an NPC
selection.
