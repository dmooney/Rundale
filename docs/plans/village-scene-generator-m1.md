# Village Scene Generator M1 Plan

## Summary

Add a deterministic recipe-driven generator that can produce ten coherent
Kilteevan Village compositor variants from the current sprite library.

## Steps

1. Done: added `mods/rundale/scene-recipes/kilteevan-village-variants.json`
   with ten named variants, descriptions, and deterministic composition knobs.
2. Done: added a visual-app generator module/CLI that reads the recipe and
   source `scenes.json`, clones `kilteevan-village`, applies layer/hotspot/slot
   transforms, and emits a scene-index-shaped variant pack.
3. Done: added tests proving exactly ten unique variants, existing PNG-only
   assets, unique z ordering, bounded interaction geometry, and preserved
   reusable kit layer families.
4. Done: added a package script that prints a variant summary and can write
   generated output for proof bundles without mutating
   `mods/rundale/scenes.json`.
5. Done: ran visual checks/tests, targeted Rust scene tests, the gameplay
   fixture, generator proof output, evidence, and judge before PR.

## Status Report

Implemented in the current branch:

- The recipe emits ten variants: Rain-Rutted Lane, Market Morning, Well
  Gathering, Bridge Stream Bank, Cottage Gardens, Signpost Crossroads Pull,
  Cart And Stone, Smoke At Dusk, Spring Hedgerows, and Wind-Bent West.
- Each generated scene keeps 77 compositor layers, 54 kit layers, and 41 M2 kit
  layers while changing 26 to 54 layer placements/prominence values.
- The generator assigns unique slugs and synthetic location IDs so the ten-pack
  avoids the duplicate scene ID hardening that protects normal scene indexes.
- The generated output keeps `plate`, `underlay`, hotspots, slots, activation
  data inputs, sprite readiness, and the existing PNG atom asset contract.
- The proof bundle contains generated pack, summary, per-variant atom-audit
  JSON, and a live gameplay transcript proving the base scene path still works.

## Tests

- `npm --prefix parish/apps/visual run check`
- `npm --prefix parish/apps/visual test`
- `npm --prefix parish/apps/visual run build`
- `npm --prefix parish/apps/visual run generate:village-variants -- --summary`
- `cargo test --manifest-path parish/Cargo.toml -p parish-mod scenes --all-targets`
- `cargo test --manifest-path parish/Cargo.toml -p parish-core scene_state --all-targets`
- `cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/fixtures/play_village-scene-generator-m1.txt`

## Open Follow-Ups

- Runtime variant selection and preview UI are deliberately out of scope for
  M1; generated packs are tooling artifacts.
- Later art passes should add more sprite alternatives per family so variant
  differences can be richer without over-transforming the same atoms.
- A future preview command should render side-by-side screenshots/contact sheets
  for selected generated variants so art direction can be judged visually, not
  only structurally.
