# Village Chunk Terrain Renderer M6 Plan

## Implementation

1. Add a `--chunk-render-mode sprites` option to the village layout generator while preserving existing raster output as the default compatibility path. Done.
2. Generate deterministic transparent PNG chunk assets from the terrain chunk grammar. Assets should be reusable by template/class/variant rather than one full-stage raster per layout. Done.
3. Convert each chunk map into compositor layers with `terrain_chunk_*` metadata, correct isometric cell positions, deterministic z ordering, and asset references. Done.
4. Keep a muted generated base ground layer only for natural fill; path, water, bank, bridge-adjacent, and detail variation should be visible chunk sprite layers. Done.
5. Extend validation to reject missing chunk assets, duplicate terrain chunk layer source ids, missing/invalid chunk metadata, and broken coverage/collision summaries. Done.
6. Extend generated summaries with chunk-sprite render metrics: mode, layer count, asset count, class counts, path/water coverage, missing assets, collisions, and signature. Done.
7. Update the proof screenshot script for M6 and produce ten desktop screenshots plus one mobile screenshot and contact sheet. Done.
8. Run visual checks/build/tests, Rust scene tests, live fallback fixture, `just agent-check`, then commit/push and attach proof to PR #1605. In progress.

## Status

Done for M6:

- `--chunk-render-mode sprites` writes generated terrain chunk PNGs plus generated ground-fill PNGs under the proof asset directory.
- Sprite-mode scenes render `terrain-ground-fill` first, then independent chunk sprite layers for visible path, water, bank, bridge, and detail terrain.
- Layer metadata records `terrain_chunk_id`, class, template, ports, mask, and variant seed.
- Summaries record chunk sprite render mode, layer count, asset count, class counts, path/water coverage, bridge under-span coverage, collision count, missing asset count, and signatures.
- Validation rejects missing chunk assets, duplicate chunk source layers, and broken coverage metrics.
- Legacy visual-water exclusion masks are not rendered as bank sprites unless tied to a real configured waterway, preventing dry layouts from showing debug-like bank grids.

Remaining after M6:

- Replace proof-grade generated chunk art with GPT-image-generated or curated pixel-art chunk families.
- Improve terrain blending at chunk seams while keeping chunk masks/ports authoritative.
- Add an optional debug overlay for ports/masks that can be toggled in proof screenshots without appearing in the first-read game view.

## Verification Commands

```sh
npm --prefix parish/apps/visual run generate:village-layouts -- --summary --out .proofs/village-chunk-terrain-renderer-m6/generated-layout-pack.json --summary-out .proofs/village-chunk-terrain-renderer-m6/generated-layout-summary.json --asset-out .proofs/village-chunk-terrain-renderer-m6/generated-assets --chunk-map-out .proofs/village-chunk-terrain-renderer-m6/generated-chunk-map.json --chunk-render-mode sprites
node --test parish/apps/visual/scripts/generate-village-layouts.test.mjs
npm --prefix parish/apps/visual run check
npm --prefix parish/apps/visual test
npm --prefix parish/apps/visual run build
cargo test --manifest-path parish/Cargo.toml -p parish-mod scenes --all-targets
cargo test --manifest-path parish/Cargo.toml -p parish-core scene_state --all-targets
cargo test --manifest-path parish/Cargo.toml -p parish-server scene --all-targets
cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/fixtures/play_village-chunk-terrain-renderer-m6.txt
just agent-check
```

## Expected Commit Shape

- `feat: render village terrain chunk sprites`
- `test: validate village chunk sprite compositor`
- `docs: record chunk terrain renderer proof`
