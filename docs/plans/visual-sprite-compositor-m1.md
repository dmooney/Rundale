# Visual Sprite Compositor M1 Plan

## Steps

1. Generate or derive transparent raster PNG atoms in the target pixel-art
   style and store them under
   `mods/rundale/assets/scenes/kilteevan-village/atoms/`.
2. Register those atoms in `mods/rundale/scenes.json` and replace Kilteevan's
   live single-plate layer with an ordered stack of atom layers.
3. Keep `plate`/`underlay` as fallback/reference fields, but ensure Pixi does
   not render them when compositor layers are present.
4. Add focused tests that Kilteevan exposes a multi-layer PNG atom stack through
   `/scene` and that the visual build/runtime path remains clean.
5. Run the fixture, visual checks, targeted Rust tests, live browser proof, and
   update `.proofs/visual-sprite-compositor-m1/` evidence and judge files.

## Test Targets

- `npm --prefix parish/apps/visual run check`
- `npm --prefix parish/apps/visual run test`
- `npm --prefix parish/apps/visual run build`
- `CARGO_TARGET_DIR=target RUSTC_WRAPPER= cargo test --manifest-path parish/Cargo.toml -p parish-core scene --all-targets`
- `CARGO_TARGET_DIR=target RUSTC_WRAPPER= cargo test --manifest-path parish/Cargo.toml -p parish-mod scenes --all-targets`
- Script fixture:
  `CARGO_TARGET_DIR=target RUSTC_WRAPPER= cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/fixtures/play_visual-sprite-compositor-m1.txt`
- Live browser screenshots at desktop 1440x900 and mobile.
