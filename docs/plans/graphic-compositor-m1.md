# Graphic Compositor M1 Plan

> Status: Review draft · Parent plan:
> [Interactive Parish Diorama — Runtime Compositor Implementation](parish-diorama-implementation.md)

## Summary

Implement the first additive compositor migration on `graphic`: extend the
scene schema and shared scene-state output while preserving all current
plate-based clients.

## Steps

1. Extend `parish-mod` scene types with compositor fields (`native_size`,
   `assets`, `layers`, `underlay`, runtime labels) using serde defaults so old
   scene files continue to parse.
2. Add validation for required asset path safety, duplicate ids, coordinate
   ranges, z-order sanity, layer asset references, and label budgets; keep
   optional unknown world/NPC references as warnings.
3. Extend `parish-core` scene-state types and builders to emit compositor
   fields while preserving `plate_url`, `hotspots`, `slots`, `npcs`, and
   `overflow_npcs`.
4. Update `/scene` text output so proof fixtures can observe native size,
   underlay, ordered layers, layer labels, legacy hotspots, slots, and overflow.
5. Migrate `mods/rundale/scenes.json` to compositor data for Kilteevan Village,
   The Crossroads, and Darcy's Pub. Keep the existing plates as transitional
   underlays or compatibility plates.
6. Keep `parish/apps/ui` and `parish/apps/visual` behavior compatible in M1;
   only update tests or type definitions needed for the additive response
   shape.

## Tests

- `CARGO_TARGET_DIR=target RUSTC_WRAPPER= cargo test -p parish-mod scenes --all-targets`
- `CARGO_TARGET_DIR=target RUSTC_WRAPPER= cargo test -p parish-core scene_state --all-targets`
- `CARGO_TARGET_DIR=target RUSTC_WRAPPER= cargo test -p parish-server scene_state_route --all-targets`
- `npm --prefix parish/apps/visual run check`
- `npm --prefix parish/apps/visual run test`
- Verification fixture:
  `cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/fixtures/play_graphic-compositor-m1.txt`

## Proof

After implementation, capture the verification fixture output as
`.proofs/graphic-compositor-m1/transcript.txt`, write evidence mapping each
acceptance criterion to transcript lines, write the required judge verdict
lines, and run `just agent-check`.
