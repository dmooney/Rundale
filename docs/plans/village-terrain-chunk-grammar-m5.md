# Village Terrain Chunk Grammar M5 Plan

## Implementation

1. Add a generated terrain chunk grammar to the layout generator with template ids for path straight/bend/fork/end, water straight/bend/end, bank edge/corner, bridge approach, dry ground, and sparse detail. Done.
2. Build a chunk map from the existing hidden grid terrain model. Each chunk records cell coordinates, class, template id, ports, mask flags, source path/waterway/bridge id where applicable, and deterministic variant seed. Done.
3. Add `--chunk-map-out` to `generate-village-layouts.mjs` and include chunk summary metrics in generated layout summaries. Done.
4. Validate chunk maps before pack output: duplicate ids, missing templates, disconnected path ports, disconnected water ports, bridge chunks without water below, and object/NPC/cottage footprint collisions must fail. Done.
5. Keep M4 raster terrain output as the proof renderer for now, while recording chunk coverage metrics beside raster metrics. Done.
6. Extend `generate-village-layouts.test.mjs` with positive chunk-map determinism and negative chunk validation cases. Done.
7. Update the screenshot proof script for M5 to copy/render the chunk-mode pack, emit 10 desktop screenshots plus one mobile screenshot, and generate a contact sheet. Done.
8. Run visual checks/build/tests, Rust scene tests, live fixture, `just agent-check`, and attach the proof to PR #1605 or a follow-up PR if the current draft has already landed. Done for local verification; attach after commit/push.

## Status

Done for M5:

- `--chunk-map-out` writes a `generated-chunk-map.json` bundle with one chunk map per generated village layout.
- Chunk maps include deterministic grammar/template metadata, per-cell chunk records, ports, masks, source ids, variant seeds, bridge under-span records, collision summaries, class counts, and template counts.
- Generated layout summaries now include chunk signatures, chunk counts, class/template counts, path/water component counts, bridge under-span counts, collision counts, and grammar signatures.
- Chunk validation rejects duplicate chunk ids, missing templates, port mismatches, disconnected path/water chunk networks, bridge records without water under-span cells, bridge under-span cells missing water chunks, and object/NPC/cottage chunk-mask collisions.
- The stricter grid chunk validation caught and fixed `forked-green`'s `west-bank` NPC slot, which occupied a water cell at chunk resolution.

Remaining after M5:

- Replace proof-grade procedural raster terrain with actual chunk/tile art selected from the chunk grammar.
- Add a visual overlay/debug export for chunk maps so reviewers can inspect ports/masks directly on screenshots.
- Feed GPT-image-generated terrain chunks into the template catalog, including multiple variants per template and stronger mask/occlusion metadata.

## Verification Commands

```sh
npm --prefix parish/apps/visual run generate:village-layouts -- --summary --out .proofs/village-terrain-chunk-grammar-m5/generated-layout-pack.json --summary-out .proofs/village-terrain-chunk-grammar-m5/generated-layout-summary.json --asset-out .proofs/village-terrain-chunk-grammar-m5/generated-assets --chunk-map-out .proofs/village-terrain-chunk-grammar-m5/generated-chunk-map.json
node --test parish/apps/visual/scripts/generate-village-layouts.test.mjs
npm --prefix parish/apps/visual run check
npm --prefix parish/apps/visual test
npm --prefix parish/apps/visual run build
cargo test --manifest-path parish/Cargo.toml -p parish-mod scenes --all-targets
cargo test --manifest-path parish/Cargo.toml -p parish-core scene_state --all-targets
cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/fixtures/play_village-terrain-chunk-grammar-m5.txt
just agent-check
```

## Expected Commit Shape

- `feat: add village terrain chunk maps`
- `test: validate generated terrain chunk grammar`
- `docs: record terrain chunk grammar proof`
