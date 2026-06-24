# Visual Kilteevan Atom Polish M3 Plan

## Steps

1. Inspect the current Kilteevan atom PNGs for rough edges, duplicated smoke,
   and pasted-on object contact.
2. Add small transparent PNG integration atoms as needed, such as
   `contact-shadows.png`, using raster generation or local image processing.
3. Register the new atoms in `mods/rundale/scenes.json` and place them below
   buildings/props but above the terrain base.
4. Remove or reduce baked smoke artifacts where a separate compositor smoke
   layer already exists.
5. Regenerate desktop/mobile screenshots and live click proof. Update
   `.proofs/visual-kilteevan-atom-polish-m3/` evidence and judge.

## Test Targets

- `npm --prefix parish/apps/visual run check`
- `npm --prefix parish/apps/visual run test`
- `npm --prefix parish/apps/visual run build`
- `CARGO_TARGET_DIR=target RUSTC_WRAPPER= cargo test --manifest-path parish/Cargo.toml -p parish-mod real_rundale_kilteevan_uses_layered_png_atoms --all-targets`
- `CARGO_TARGET_DIR=target RUSTC_WRAPPER= cargo test --manifest-path parish/Cargo.toml -p parish-server scene_state_route_exposes_kilteevan_png_compositor_layers --all-targets`
- Script fixture:
  `CARGO_TARGET_DIR=target RUSTC_WRAPPER= cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/fixtures/play_visual-kilteevan-atom-polish-m3.txt`
- Live browser screenshots at desktop `1440x900` and mobile.
