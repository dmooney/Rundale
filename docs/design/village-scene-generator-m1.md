# Village Scene Generator M1 Design Note

> Status: Implemented on `codex/village-scene-generator-m1`.
> Task: `village-scene-generator-m1`.

## Player And Author Experience

The player still launches into a polished Kilteevan Village scene. The authoring
story changes: instead of hand-editing one layer stack for every experiment, an
author writes a compact recipe describing ten moods and layout emphases, then a
deterministic generator emits ten scene variants that the Pixi compositor can
render as normal `SceneState.layers`.

The target visual language stays high 3/4 isometric/oblique: readable ground
planes, roof tops, sprite foot anchors, y-depth sorting, and no low-horizon
Crossroads-style camera for village variants.

## Affected Subsystems

- `mods/rundale`: adds a variant recipe alongside the existing scene data.
- `parish/apps/visual`: adds generator/audit tooling and regression tests for
  generated compositor scenes.
- `parish-core` / `parish-mod`: no breaking schema change is planned; generated
  scenes must remain compatible with the additive scene contract already served
  by `/scene`.

## Data Model

The recipe is a source-control artifact, not generated runtime state. It names
the source scene slug, declares ten variants, and gives each variant a plain
description plus deterministic composition knobs such as family offsets,
opacity/density emphasis, hotspot/slot shifts, and optional layer prominence.

The generator clones the source scene, applies those knobs to existing layer
instances, and emits a scene-index-shaped JSON object for tooling:

- `assets`: copied from `mods/rundale/scenes.json`.
- `scenes`: ten generated Kilteevan variant scene objects.
- `summary`: stable metadata for proof output.

The live base `kilteevan-village` scene remains hand-selected until a later
milestone chooses a runtime variant-selection policy.

## Observable Signals

The important proof is structural before it is visual: ten generated scenes, ten
unique signatures, no missing assets, no SVG layers, preserved hotspots/slots,
and at least forty reusable kit layers per variant. A later visual proof can
select individual variants for screenshots once the generation path is stable.

This milestone intentionally keeps generation deterministic and local. It is a
bridge from a single good village composition to a Factorio-like sprite
compositor workflow where layouts can be varied by recipe without baking a full
background render for each option.

## Implementation Status

M1 adds a committed recipe at
`mods/rundale/scene-recipes/kilteevan-village-variants.json` with ten named
Kilteevan briefs. Each brief combines a human-readable description with
deterministic transforms for asset families, individual layers, hotspots, and
NPC slots.

The generator lives at
`parish/apps/visual/scripts/generate-village-variants.mjs`. It reads the recipe
and `mods/rundale/scenes.json`, clones the live `kilteevan-village` compositor
scene, applies the configured transforms, assigns unique synthetic location IDs,
and emits a scene-index-shaped pack with copied `assets`, `sprites`,
`fallback_sprites`, ten generated `scenes`, and a stable `summary`.

The generated pack is tooling/proof output, not a runtime scene-selection
policy. The live base village scene is unchanged; the current client still
serves the hand-selected Kilteevan scene while authoring workflows can now
generate ten alternate sprite-composited layouts for inspection.

Regression coverage in
`parish/apps/visual/scripts/generate-village-variants.test.mjs` proves all ten
generated variants keep the compositor contract, use existing PNG scene atoms,
avoid duplicate z values, keep hotspots and slots in bounds, retain at least
forty M2 kit layers, and pass the existing atom audit hook.
