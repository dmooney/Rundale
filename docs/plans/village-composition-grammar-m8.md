# Village Composition Grammar M8 Plan

## Implementation

1. Add `composition` blocks to the ten outdoor village layout configs with archetype, focal role, structure families, prop roles, NPC slot roles, and density/style tags.
2. Add composition validation helpers in `generate-village-layouts.mjs` that ensure role ids are known, focal roles are present, structure/prop families are catalog-covered, and composition signatures are sufficiently varied.
3. Thread composition metadata into generated scene summaries and aggregate pack summary metrics.
4. Use structure family assignments to vary cottage asset intent and AI catalog prompt metadata without bypassing current node/footprint validation.
5. Update `assertGeneratedPack` and focused tests to reject compositionally samey but physically valid packs.
6. Regenerate proof artifacts, screenshots, contact sheet, transcript, evidence, and judge.
7. Run the visual/Rust/live gates, commit/push, and attach the M8 proof to PR #1605.

## Verification Commands

```sh
npm --prefix parish/apps/visual run generate:village-layouts -- --summary --out .proofs/village-composition-grammar-m8/generated-layout-pack.json --summary-out .proofs/village-composition-grammar-m8/generated-layout-summary.json --asset-out .proofs/village-composition-grammar-m8/generated-assets --chunk-map-out .proofs/village-composition-grammar-m8/generated-chunk-map.json --asset-catalog-out .proofs/village-composition-grammar-m8/generated-asset-catalog.json --chunk-render-mode sprites
node --test parish/apps/visual/scripts/generate-village-layouts.test.mjs
npm --prefix parish/apps/visual run check
npm --prefix parish/apps/visual test
npm --prefix parish/apps/visual run build
cargo test --manifest-path parish/Cargo.toml -p parish-mod scenes --all-targets
cargo test --manifest-path parish/Cargo.toml -p parish-core scene_state --all-targets
cargo test --manifest-path parish/Cargo.toml -p parish-server scene --all-targets
cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/fixtures/play_village-composition-grammar-m8.txt
just agent-check
```

## Expected Commit Shape

- `feat: add village composition grammar`
- `test: enforce generated village composition variety`
- `docs: record village composition proof`

## Status Notes

- Implemented composition validation and summaries in the village layout generator.
- Added all ten recipe composition blocks plus role-specific prop families: market planks, gates, peat stacks, wash lines, barrel clusters, garden plots, bridge spans, stone wall runs, wells, carts, and signposts.
- Added focused tests for positive variety metrics and negative samey/contradictory configs.
- Proof generation produces both chunk-sprite terrain screenshots for atom-catalog coverage and raster-terrain screenshots for the better final visual direction.
- Next visual milestone should improve the terrain raster art pass and connect it to generated AI terrain plates, while leaving manmade objects/NPCs as independently composited sprites with validated footprints.
