# Village Composition Grammar M8

## Purpose

M7 made the asset production contract explicit, but the generated screenshots still lean on the same small set of staged objects. M8 makes composition a first-class part of the outdoor village config: each layout declares what kind of settlement it is, which structure families it uses, which object roles are focal or absent, and what NPC slot roles belong there.

## Player Experience

Players should see ten villages that feel like related places in the same parish, not ten rearrangements of a single postcard. One may read as a bridge hamlet, another as a well square, another as a farm track, market lane, chapel road, river bend, or overgrown hollow. The paths, water, and bridges still make physical sense, but the manmade scene logic varies enough that a player expects different interactions.

## Affected Subsystems

- `mods/rundale/scene-recipes/outdoor-village-layouts.json`: add composition metadata to the existing layout records and ensure used role/family ids are covered by `ai_asset_strategy`.
- `parish/apps/visual/scripts/generate-village-layouts.mjs`: validate composition grammar, derive per-layout and aggregate composition summaries, thread role/family choices into generated layers/prompts, and reject samey packs.
- `parish/apps/visual/scripts/generate-village-layouts.test.mjs`: assert variation metrics, negative validation cases, and AI catalog coverage.
- `.proofs/village-composition-grammar-m8/`: generated pack, summary, catalog, screenshots, transcript, evidence, and judge.
- `parish/testing/fixtures/play_village-composition-grammar-m8.txt`: live fallback proof.

## Data Model

Each layout should gain a `composition` block with stable role ids:

```json
{
  "composition": {
    "archetype": "bridge-hamlet",
    "focal_role": "bridge",
    "structure_families": [
      "whitewashed thatch cottage left-facing",
      "small byre"
    ],
    "prop_roles": ["bridge", "well", "cart-layby", "signpost"],
    "npc_slot_roles": ["traveller", "well-gossip", "bridge-watch"],
    "density_tags": ["wet", "clustered", "stream-bank"]
  }
}
```

The existing geometry remains authoritative. `composition` describes what a node/prop/site means; it does not place objects directly. Any role that names a prop or structure must resolve to an existing physically validated node/site/footprint.

## Validation

Validation should fail when composition metadata is missing, references unknown families, claims a focal role that is not present, repeats signatures too often, or uses a prop/structure family not represented in the AI asset strategy. Physical validation still runs before and after composition validation so variety cannot mask broken waterways, bridges, carts, cottages, or NPC slots.

## Status

Implemented on `codex/village-scene-generator-m1` for PR #1605. The recipe now declares ten composition archetypes with structure families, focal roles, prop role mixes, NPC slot roles, and density tags. The generator emits per-layout composition signatures plus aggregate metrics, and rejects physically valid but compositionally samey configs.

The proof renders two useful visual modes:

- Chunk-sprite terrain mode proves the Factorio-like terrain atom catalog and produced 349 terrain chunk sprite requests for M8. This is still visibly tiled in screenshots and should not be treated as the final visual surface.
- Raster-terrain mode uses a full natural ground/water background per layout, then composites manmade sprites, props, NPCs, smoke, labels, and hotspots above it. This is the more satisfying visual direction for scaled AI generation because river/path continuity belongs to one terrain pass while cottages, bridges, carts, wells, signs, props, and NPC atoms remain independently composited.

Remaining visual issues are art-direction work, not layout-logic blockers: path and water raster textures are too soft/simple, mobile framing crops aggressively, and the current cottage sprites are still placeholders for the future multi-family AI asset catalog.

## Feature Flag

No runtime feature flag is needed while M8 remains proof tooling. If composition-generated packs are promoted into live `scenes.json`, gate that rollout behind a default-on flag such as `visual-village-composition-grammar`.
