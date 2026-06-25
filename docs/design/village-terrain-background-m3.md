# Village Terrain Background M3

## Purpose

M2 made village layouts physically safer by adding a hidden isometric grid, path/water validation, bridge constraints, prefab footprints, and atom-based scene output. It still preserved a full-stage Kilteevan ground base, so the ten variants look more like alternate prop arrangements on one painting than like generated places. M3 moves natural terrain into the generator: the configuration describes terrain topology first, the generator builds a varied terrain/background pass from that topology, and only then does the compositor place cottages, bridges, walls, carts, wells, signs, foliage, smoke, and NPC slots.

The player experience target is a full-screen adventure-game scene that resembles a hand-authored Stardew/Factorio-like isometric outdoor location: connected paths, continuous water, correctly placed bridges, readable cottage doors, props on dry ground, and NPCs standing where people could stand.

## Affected Subsystems

- `mods/rundale/scene-recipes/outdoor-village-layouts.json`: add or formalize terrain profile data, terrain variation tags, and any per-layout background/underpaint controls.
- `mods/rundale/scenes.json`: continue using PNG atoms, but distinguish full-stage calibration art from reusable terrain/object atoms.
- `parish/apps/visual/scripts/generate-village-layouts.mjs`: generate terrain/background layers from layout topology before water, roads, bridges, and objects; summarize terrain metrics.
- `parish/apps/visual/scripts/generate-village-layouts.test.mjs`: assert terrain profile uniqueness, demoted shared base usage, negative validation cases, and retained physical coherence.
- `parish/apps/visual/src/**`: renderer compatibility should remain unchanged unless the existing layer semantics prevent correct terrain/background rendering.
- `.proofs/village-terrain-background-m3/`: capture generated pack, summary, screenshots, evidence, and judge verdict.
- `parish/testing/fixtures/play_village-terrain-background-m3.txt`: live command fallback proof for the playable slice.

No Rust data model change is intended for M3. The runtime scene contract remains additive and compatible: `SceneState.layers`, `native_size`, hotspots, slots, NPC sprites, labels, and legacy plate fields continue to work.

## Data Model

The recipe should grow from object placement into a terrain-first scene description:

- `terrain_profiles`: reusable style/topology profiles such as `wet_stream_crossing`, `dry_green`, `market_lane`, `chapel_rise`, and `forked_bank`.
- Per-layout terrain inputs: grade/rise direction, base grass/mud density, path width, bank width, wetness, puddle density, vegetation clusters, and lighting/weather overlays.
- Derived summary fields: `terrain_signature`, `terrain_profile`, `terrain_layer_count`, `terrain_underpaint_layer_count`, `shared_ground_base_opacity`, and coverage/continuity metrics.

Generated scene JSON should still be ordinary compositor output. Terrain metadata can live in the recipe and generator summary until the client needs it directly.

## Rendering Model

The target stack is:

1. Optional low-opacity calibration/backdrop layer, if still useful during migration.
2. Generated terrain/background pass: ground, grass, mud, water, banks, road underpaint, puddles, broad shadow/lighting, and foreground grade hints.
3. Constructed objects: cottages, walls, bridge decks, carts, wells, signs, market planks, gates, smoke, and foreground foliage.
4. NPC sprites and interaction affordances.
5. Subordinate caption/log/input overlay in the full-screen visual client.

Water should never be visually cut to make room for a bridge. Rivers and streams remain continuous in the terrain pass; bridge sprites sit above them and supply the walkable deck. This matches the physical validation model and prevents broken downstream continuity.

## Scalable AI Asset Direction

AI generation should expand the sprite catalog, not replace the compositor. The generator needs many compatible atoms:

- Natural terrain: multiple grass/mud/road/bank/water chunks with consistent pixel density, palette, lighting, contact shadows, and high 3/4 isometric perspective.
- Cottages: left/right/gable/byre variants with door anchors, chimney-opening anchors, footprints, occlusion masks, and roof contact shadows.
- Props: carts, wells, signposts, fences, walls, gates, barrels, peat, market planks, and bridges with footprints, ports, and forbidden-terrain masks.
- NPCs: atom assemblies for body stance, head, hair, hat, shawl, coat, apron, trousers/skirt, boots, held tools, and carried objects, all sharing foot anchors and scale rules.

This is the "could not be made on a normal budget" direction: the LLM/image model produces many style-locked atoms and metadata, while deterministic topology and validation decide where atoms are allowed to exist.

## Observable Signals

The generator summary should prove:

- exactly ten layouts;
- unique scene and topology signatures;
- unique or intentionally varied terrain signatures;
- connected road cells;
- water components matching declared waterways;
- continuous water beneath bridge spans;
- zero rendered-water prop collisions;
- zero invalid NPC/cottage/prop anchors;
- shared base layer absent or below the accepted dominance threshold.

The screenshots should prove what metrics cannot: that the first viewport reads as varied playable pixel-art villages, not as one full-scene painting with different props.

## Feature Flag

No runtime feature flag is planned for M3 because the change is a visual-generator/proof milestone, not a new engine behavior path. If generated scenes are later wired into the live mod selection by default, that integration should use a default-on flag such as `visual-generated-village-scenes`.
