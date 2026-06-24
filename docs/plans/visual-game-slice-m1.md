# Visual Game Slice M1 Plan

> Status: Approved for implementation. Parent branch: `graphic`.

## Summary

Build a proof-quality, full-screen graphical slice on top of
`graphic-compositor-m1`, keeping the compositor contract additive while making
`parish/apps/visual` feel like a playable adventure game.

## Steps

1. Add acceptance criteria, fixture, design note, and implementation plan before
   code changes.
2. Harden scene validation in `parish-mod` for duplicate scene locations/slugs,
   duplicate hotspot ids, duplicate slot ids, and duplicate NPC sprite ids.
3. Extend the shared scene-state hotspot view with deterministic activation
   hints and render those hints in `/scene`.
4. Replace Crossroads reuse in Kilteevan Village with distinct curated visual
   atoms under `mods/rundale/assets/scenes/kilteevan-village/`.
5. Add a PixiJS-based visual renderer that draws ordered layers, labels,
   hotspots, weather/lighting overlays, NPC sprites, hover/click feedback, and
   scene transitions.
6. Replace the visual app shell with a full-screen canvas, bottom caption/log,
   compact command input, and hidden settings panel.
7. Run backend, visual, fixture, and live screenshot verification; write proof
   evidence and judge files.

## Tests

- `cargo test --manifest-path parish/Cargo.toml -p parish-mod scenes --all-targets`
- `cargo test --manifest-path parish/Cargo.toml -p parish-core scene_state --all-targets`
- `cargo test --manifest-path parish/Cargo.toml -p parish-server scene_state_route --all-targets`
- `npm --prefix parish/apps/visual run check`
- `npm --prefix parish/apps/visual run test`
- `npm --prefix parish/apps/visual run build`
- `cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/fixtures/play_visual-game-slice-m1.txt`
- Live browser screenshots at 1440x900 and mobile widths.
