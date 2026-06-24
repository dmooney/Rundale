# Visual Kilteevan Art M2 Plan

## Steps

1. Generate or derive cleaner raster pixel-art material for Kilteevan, using
   the provided reference direction: damp 1820s rural Ireland, coherent 3/4
   perspective, muted natural palette, and no text baked into the art.
2. Keep the compositor contract: final project assets are PNG atoms under
   `mods/rundale/assets/scenes/kilteevan-village/atoms/`; no SVG placeholder
   target assets.
3. Replace the weakest current atoms first: ground/base treatment and scene
   composition layers that create visible tiling or cutout artifacts.
4. Adjust `mods/rundale/scenes.json` layer placement only as needed to improve
   the first viewport while preserving existing hotspot commands.
5. Run the visual client checks, targeted scene tests, script fixture, and live
   browser screenshot/click proof. Record evidence in
   `.proofs/visual-kilteevan-art-m2/`.

## Test Targets

- `npm --prefix parish/apps/visual run check`
- `npm --prefix parish/apps/visual run test`
- `npm --prefix parish/apps/visual run build`
- `CARGO_TARGET_DIR=target RUSTC_WRAPPER= cargo test --manifest-path parish/Cargo.toml -p parish-mod real_rundale_kilteevan_uses_layered_png_atoms --all-targets`
- `CARGO_TARGET_DIR=target RUSTC_WRAPPER= cargo test --manifest-path parish/Cargo.toml -p parish-server scene_state_route_exposes_kilteevan_png_compositor_layers --all-targets`
- Script fixture:
  `CARGO_TARGET_DIR=target RUSTC_WRAPPER= cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/fixtures/play_visual-kilteevan-art-m2.txt`
- Live browser screenshots at desktop `1440x900` and mobile.
