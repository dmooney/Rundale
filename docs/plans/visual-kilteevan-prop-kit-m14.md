# visual-kilteevan-prop-kit-m14

1. Add the proof bundle, deterministic script fixture, design note, and plan.
2. Inspect Kilteevan's existing broad atoms and derive a small kit directory
   from compatible wall, foliage, and terrain/road detail crops.
3. Register the new kit assets in `mods/rundale/scenes.json` and place repeated
   layers over the existing broad composition with conservative opacity.
4. Refactor the scene atom auditor from a Crossroads-only check into a
   multi-scene audit with per-scene required reusable kit families.
5. Update visual tests for the new audit summary and run the backend/schema,
   visual, script, browser, and proof-gate checks.
