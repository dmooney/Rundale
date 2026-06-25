# Village AI Asset Catalog M7 Plan

## Implementation

1. Add catalog generation helpers to `generate-village-layouts.mjs` that consume the recipe, terrain chunk grammar, chunk maps, generated chunk sprite metadata, and source scene asset families.
2. Add `--asset-catalog-out` to the generator CLI. When present, emit a JSON catalog beside the generated pack/chunk map.
3. Generate terrain requests from reusable chunk sprite assets, with prompts and metadata tied to class/template/ports/masks/variant seeds.
4. Generate cottage/prop requests from `ai_asset_strategy.cottage_families` and `prop_families`, including anchors, footprints, compatibility tags, and setting constraints.
5. Generate NPC atom requests from `ai_asset_strategy.npc_atom_families`, plus deterministic example NPC assemblies with required atom slots and layer ordering.
6. Add `assertAiAssetCatalog` validation for duplicate ids, missing prompts, missing style tags, missing output paths, missing anchors/masks, terrain-template mismatches, and incomplete NPC assemblies.
7. Add focused tests for deterministic catalog generation and negative validation cases.
8. Generate proof artifacts, run visual/Rust/live checks, commit/push, and attach proof to PR #1605.

## Verification Commands

```sh
npm --prefix parish/apps/visual run generate:village-layouts -- --summary --out .proofs/village-ai-asset-catalog-m7/generated-layout-pack.json --summary-out .proofs/village-ai-asset-catalog-m7/generated-layout-summary.json --asset-out .proofs/village-ai-asset-catalog-m7/generated-assets --chunk-map-out .proofs/village-ai-asset-catalog-m7/generated-chunk-map.json --asset-catalog-out .proofs/village-ai-asset-catalog-m7/generated-asset-catalog.json --chunk-render-mode sprites
node --test parish/apps/visual/scripts/generate-village-layouts.test.mjs
npm --prefix parish/apps/visual run check
npm --prefix parish/apps/visual test
npm --prefix parish/apps/visual run build
cargo test --manifest-path parish/Cargo.toml -p parish-mod scenes --all-targets
cargo test --manifest-path parish/Cargo.toml -p parish-core scene_state --all-targets
cargo test --manifest-path parish/Cargo.toml -p parish-server scene --all-targets
cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/fixtures/play_village-ai-asset-catalog-m7.txt
just agent-check
```

## Expected Commit Shape

- `feat: emit village AI asset catalog`
- `test: validate AI asset catalog coverage`
- `docs: record AI asset catalog proof`
