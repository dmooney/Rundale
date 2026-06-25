# Village Layout Compositor M2 Plan

## Implementation

1. Add a terrain-first outdoor village layout recipe under `mods/rundale/scene-recipes/`, including the hidden isometric grid and prefab catalog.
2. Add a visual app generator script that reads the recipe and `mods/rundale/scenes.json`, emits ten compositor-compatible scene variants, and writes a summary with topology validation results.
3. Validate path connectivity, exit reachability, grid road/water components, bridge prefab coverage, continuous water under bridges, cart rendered-water collision masks, walkable cottage/NPC anchors, duplicate ids, and reusable atom counts.
4. Add focused Node tests for success and negative physical-coherence cases.
5. Add a package script so the generator is easy to run in proof and future tooling.
6. Run the visual checks/build/tests and the relevant Rust scene tests.
7. Produce `.proofs/village-layout-compositor-m2/evidence.md` plus a judge verdict after commands complete.

## Verification Commands

```sh
npm --prefix parish/apps/visual run generate:village-layouts -- --summary --out .proofs/village-layout-compositor-m2/generated-layout-pack.json --summary-out .proofs/village-layout-compositor-m2/generated-layout-summary.json
node --test parish/apps/visual/scripts/generate-village-layouts.test.mjs
npm --prefix parish/apps/visual run check
npm --prefix parish/apps/visual test
npm --prefix parish/apps/visual run build
cargo test --manifest-path parish/Cargo.toml -p parish-mod scenes --all-targets
cargo test --manifest-path parish/Cargo.toml -p parish-core scene_state --all-targets
cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/fixtures/play_village-layout-compositor-m2.txt
just agent-check
```
