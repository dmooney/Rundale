# Village Terrain Background M3 Plan

## Implementation

1. Extend the village layout recipe with terrain/background configuration: named terrain profiles, grade/wetness/vegetation parameters, per-layout terrain tags, and base-layer migration controls.
2. Update `generate-village-layouts.mjs` so each layout emits terrain-underpaint layers before water, paths, bridges, and objects. Demote or remove the stretched `kilteevan-ground-base` layer from generated variants.
3. Add terrain metrics to the generated summary: terrain profile, terrain signature, terrain layer counts, shared base opacity, and relevant continuity/coverage values.
4. Keep the existing physical validation and strengthen it where terrain generation creates new risks: roads connected, waterways continuous, bridge crossings valid, carts/props dry, NPC slots valid, and cottage doors connected to paths.
5. Add tests for positive generation plus negative failures: duplicate terrain signatures, dominant shared base usage, broken water continuity, invalid bridge placement, rendered-water collision, disconnected road cells, and NPC slots on invalid terrain.
6. Generate the ten-scene pack and screenshots for desktop review. Add a contact sheet or manifest that makes terrain variation easy to compare.
7. Run the visual checks/build/tests, relevant Rust scene tests, live script fixture, atom audit, and `just agent-check`.
8. Attach the finished proof bundle to PR #1605 after evidence and judge files are written.

## Verification Commands

```sh
npm --prefix parish/apps/visual run generate:village-layouts -- --summary --out .proofs/village-terrain-background-m3/generated-layout-pack.json --summary-out .proofs/village-terrain-background-m3/generated-layout-summary.json
node --test parish/apps/visual/scripts/generate-village-layouts.test.mjs
npm --prefix parish/apps/visual run check
npm --prefix parish/apps/visual test
npm --prefix parish/apps/visual run build
npm --prefix parish/apps/visual run audit:atoms
cargo test --manifest-path parish/Cargo.toml -p parish-mod scenes --all-targets
cargo test --manifest-path parish/Cargo.toml -p parish-core scene_state --all-targets
cargo test --manifest-path parish/Cargo.toml -p parish-server scene --all-targets
cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/fixtures/play_village-terrain-background-m3.txt
just agent-check
```

## Expected Commit Shape

- `feat: add generated village terrain backgrounds`
- `test: cover generated terrain background constraints`
- `docs: record village terrain background proof`
