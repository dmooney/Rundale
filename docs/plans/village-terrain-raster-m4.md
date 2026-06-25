# Village Terrain Raster M4 Plan

## Implementation

1. Add a small PNG writer utility in the visual scripts, reusing the existing PNG parser/test patterns where possible and avoiding a new dependency unless the local toolchain cannot encode deterministic RGBA PNGs cleanly.
2. Extend `generate-village-layouts.mjs` with `--asset-out` and generated-asset URL handling so proof packs can reference generated terrain PNGs without committing binary assets.
3. Generate one full-stage terrain raster per layout from the hidden grid and terrain profile: base grass/mud, paths, water, banks, puddles, vegetation/noise, and lighting. Keep water continuous under bridges.
4. Add generated terrain assets to the pack's `assets` array and add one terrain raster layer before constructed object layers.
5. Reduce repeated broad terrain atom use in generated layouts to sparse detail only; keep cottages, bridges, walls, props, smoke, foliage, and NPCs as compositor sprites.
6. Add summary metrics for raster asset id/path, pixel hash, raster signature, native size, raster layer count, repeated terrain atom count, and water/path coverage.
7. Extend tests for deterministic byte-identical generation, duplicate raster signature rejection, missing generated asset rejection, water/path/bridge coverage, rendered-water collision preservation, and invalid anchor cases.
8. Update the proof screenshot renderer to serve generated asset paths from the proof directory and capture ten desktop screenshots plus one mobile screenshot and a contact sheet.
9. Run visual checks/build/tests, Rust scene tests, live fixture, `just agent-check`, and attach the finished proof to PR #1605.

## Status

Done for M4:

- `--asset-out` writes 10 deterministic generated PNG terrain rasters and a pack that references them as generated ground assets.
- Raster-mode scenes use one `terrain-raster` layer before constructed sprite layers; old repeated terrain underpaint layers are disabled in this mode.
- Summary metrics include raster asset id, raster signature, pixel hash, native size, raster layer count, grid-painted cell count, water/path coverage, and repeated terrain atom count.
- Tests cover repeat-run byte determinism, missing generated asset rejection, duplicate raster signature rejection, topology failures, rendered-water cart collision, NPC/cottage anchor failures, and generated PNG content checks.
- Proof screenshots render all ten layouts plus one mobile frame with `missingLayerAssets: 0`, `fallbackPlateUsed: false`, and `fallbackUnderlayUsed: false`.

Remaining after M4:

- Promote generated assets into live mod asset paths only when the visual direction is accepted; proof-pack paths are intentionally relative to `.proofs/.../generated-assets`.
- Replace the procedural terrain painter with AI-generated isometric terrain chunks or masked underpaint tiles that match the high-quality cottage/prop sprites.
- Add a mixed-root atom/raster auditor if proof packs continue to combine committed `mods/rundale/assets/...` sprites with generated proof assets.
- Add stricter pixel-level assertions around bridge-center water, dry cottage/cart/NPC footprints, and path-to-door raster alignment once the next terrain painter lands.

## Verification Commands

```sh
npm --prefix parish/apps/visual run generate:village-layouts -- --summary --out .proofs/village-terrain-raster-m4/generated-layout-pack.json --summary-out .proofs/village-terrain-raster-m4/generated-layout-summary.json --asset-out .proofs/village-terrain-raster-m4/generated-assets
node --test parish/apps/visual/scripts/generate-village-layouts.test.mjs
npm --prefix parish/apps/visual run check
npm --prefix parish/apps/visual test
npm --prefix parish/apps/visual run build
npm --prefix parish/apps/visual run audit:atoms
cargo test --manifest-path parish/Cargo.toml -p parish-mod scenes --all-targets
cargo test --manifest-path parish/Cargo.toml -p parish-core scene_state --all-targets
cargo test --manifest-path parish/Cargo.toml -p parish-server scene --all-targets
cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/fixtures/play_village-terrain-raster-m4.txt
just agent-check
```

## Expected Commit Shape

- `feat: generate village terrain rasters`
- `test: cover terrain raster determinism`
- `docs: record village terrain raster proof`
