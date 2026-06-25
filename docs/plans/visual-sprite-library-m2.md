# Visual Sprite Library M2 Plan

## Summary

Build a reusable pixel-art sprite library for Kilteevan Village and recompose
the scene in a Stardew/Factorio-like high 3/4 isometric/oblique perspective.

## Steps

1. Generate or derive transparent PNG kit atoms for road, mud, puddles, walls,
   hedges, flowers, cottage details, smoke, signs, shadows, and decals.
2. Store selected atoms under
   `mods/rundale/assets/scenes/kilteevan-village/atoms/kit/` and audit their
   alpha edges/components.
3. Recompose Kilteevan's `layers` with repeated small atom instances, keeping
   plate fields as fallback/reference only.
4. Add visual tests/audits proving repeated kit atoms, no SVG live layers, and
   no dominant non-background chunk.
5. Run fixture, visual checks, atom audit, live browser screenshots, evidence,
   judge, and final verification before opening the PR to `graphic`.

## Status Report

Implemented in the current branch:

- Created 25 new `m2-` transparent PNG kit atoms for Kilteevan Village and
  removed generated chroma-key fringe before proof capture.
- Added 41 new `m2-` layer instances, raising Kilteevan to 77 compositor layers
  and 54 total kit layers.
- Preserved the additive scene contract: legacy plate/underlay fields remain,
  while live Pixi telemetry shows the rendered compositor did not fall back to
  the plate.
- Strengthened the atom audit and regression tests so Kilteevan must keep
  repeated reusable PNG kit families and small sprite dimensions.
- Captured live desktop/mobile screenshots and click-path telemetry for
  inspect, NPC selection, Kilteevan -> Crossroads, and Crossroads -> Darcy's
  Pub.

Remaining next-step work:

- Replace more of the large Kilteevan base/local atoms with reusable terrain,
  wall, roof, and cottage-body families.
- Add authoring tooling for placing and previewing atom stacks without editing
  `scenes.json` by hand.
- Generate multiple coherent variants per atom family so repeated sprites can
  avoid obvious cloning while staying isometric.

## Tests

- `npm --prefix parish/apps/visual run check`
- `npm --prefix parish/apps/visual test`
- `npm --prefix parish/apps/visual run build`
- `npm --prefix parish/apps/visual run audit:atoms`
- `cargo test --manifest-path parish/Cargo.toml -p parish-mod scenes --all-targets`
- `cargo test --manifest-path parish/Cargo.toml -p parish-core scene_state --all-targets`
- `cargo test --manifest-path parish/Cargo.toml -p parish-server scene --all-targets`
- `cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/fixtures/play_visual-sprite-library-m2.txt`
- Live screenshots at `1440x900` and mobile width.
