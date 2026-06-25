# Village Layout Compositor M2

## Purpose

M1 proved we can emit ten deterministic scene variants, but those variants were still mostly one authored painting with layers nudged around it. M2 moves the generator to a terrain-first model: configuration describes the physical village first, an invisible isometric grid validates terrain occupancy and prefab sockets, then the compositor places road, water, bridge, cottage, prop, foliage, and NPC atoms against that topology.

The long-term target is a Rundale-scale outdoor scene system that benefits from AI asset generation: thousands of locations can share validation, layout rules, anchors, prompts, and sprite families while still looking hand-placed.

## Scene Recipe Shape

Each outdoor layout recipe declares:

- `grid`: the invisible isometric occupancy grid used for path, water, bridge, prop, and slot validation. The current renderer still emits percent-space coordinates for compatibility.
- `prefab_catalog`: semantic object contracts such as `bridge-crossing`, `cart-pullout`, `cottage`, `well-node`, `signpost-node`, and `npc-standing-slot`, each with ports, requirements, and forbidden terrain.
- `nodes`: named percent-space points for entry, exits, junctions, cottage doors, wells, banks, and gathering points.
- `paths`: edges between nodes; these become walkable ground, travel hotspots, and road sprite placement guides.
- `waterways`: ordered polylines for streams, rivers, drains, or pond edges.
- `bridges`: a path segment plus a water segment that must geometrically cross near the bridge center.
- `cottage_sites`: footprint, door anchors connected to a path, and chimney-opening sockets for smoke placement.
- `activity_anchors`: wells, carts, signs, market spots, chapel approaches, banks, and wall gates.
- `npc_slots`: named standing points tied to a path or activity anchor.
- `prop_clusters`: higher-level groups that expand into layered sprite atoms.

The generated `SceneState` remains plain compositor output: ordered `layers`, hotspots, slots, sprites, and legacy plate fields. Grid, terrain, prefab, and topology metadata live in the generator summary and proof artifacts, not in the runtime contract yet.

## Physical Validation

The generator rejects:

- disconnected path graphs;
- grid road cells split into multiple components;
- grid water cells split within a waterway;
- exits unreachable from the entry node;
- bridge centers not near both their declared path segment and water segment;
- bridge declarations whose path and water segments do not intersect;
- bridges whose waterway terminates at the bridge instead of continuing underneath and past it;
- bridge prefabs that do not cover their path/water crossing cells;
- props whose prefab footprints intersect topology water or rendered/base-art water masks;
- placements that do not resolve through a known prefab contract;
- cottages whose door anchors are too far from a path;
- cottage/NPC/activity anchors placed in water;
- duplicate scene ids/slugs, layer ids, hotspot ids, slot ids, and sprite ids;
- generated scenes with too few reusable atom layers to be a compositor proof.

This turns composition into a constraint problem instead of a screenshot problem. A pretty impossible village fails fast. The current `visual_water_exclusions` field is a transitional terrain mask for baked water still present in the Kilteevan base art; the proper next step is to generate or select a natural ground/water background per layout so collision truth and pixels come from the same terrain pass.

M2 still uses percent-space layer placement, but the direction is not arbitrary sprite scattering. Every scalable sprite family needs semantic connection data: door sockets, chimney openings, foot anchors, bridge bank contacts, wall endpoints, path contact lines, footprints, collision masks, shadow footprints, and occlusion masks. The layout solver should place and connect those sockets, then ask the renderer to draw the chosen atoms. The smoke/chimney socket in this milestone is the first small version of that contract.

This version introduces the first invisible logical isometric grid. The player never sees grid lines, and the art should still render as irregular painterly pixel sprites, but the generator validates topology on grid cells and edges: walkable cells, water cells, bridge spans, cottage footprints, door cells, NPC foot cells, and prop occupancy. A later migration should make grid nodes and prefab sockets the authoring source of truth, with percent-space nodes retained only as generated renderer coordinates. That gives us the discipline of classic isometric games without forcing a visible tile-map look.

The renderer should also separate natural terrain from constructed objects. Ground, water, mud, grass, riverbanks, road underpaint, and broad lighting should come from a coherent generated terrain/background pass, likely one raster per location or per terrain chunk. Manmade and discrete readable objects then compose on top: cottages, bridges, carts, wells, walls, signposts, doors/windows, NPCs, smoke, and foreground foliage. The M2 proof currently uses sprite atoms for both terrain accents and objects, which is useful for testing, but the cart-over-water and river-continuity issues show why natural terrain wants a unified base before object placement.

Bridge crossings should not interrupt river art. The river/stream belongs to the terrain pass and remains visually continuous under the bridge; the bridge sprite is then drawn above it with its deck and bank contacts aligned to the path graph. Walkability changes on the bridge deck, but the water layer never gets cut into disconnected pieces. This is the safest default for avoiding gaps, broken downstream continuity, and cart/bridge sprites accidentally defining the river edge.

## AI Asset Families

The compositor is deliberately designed around asset families rather than single hero images:

- Cottages: many thatch/stone/door/window/chimney variants, each exported with consistent high 3/4 perspective, scale, lighting, foot anchors, door anchors, chimney-opening anchors, and occlusion masks.
- Roads and banks: small tiles and irregular chips tagged by use (`main_lane`, `fork`, `bank_mud`, `puddle`, `wheel_rut`) so topology can choose them.
- Bridges: multiple spans with declared length, bank contact points, walkable deck segment, and compatible water angles.
- Props: wells, carts, signposts, barrels, stacks, fences, gates, peat, wash lines, and garden plots with footprint bounds and interaction anchors.
- NPCs: atom assemblies for body, head, hair, hat, shawl, coat, apron, trousers/skirt, tools, carried objects, and stance. A seed can combine atoms into millions of unique NPC sprites while preserving anchor, scale, and costume rules for 1820s rural Ireland.

The core insight is that AI generation should expand the catalog, not bypass the compositor. The layout system decides what must exist physically; generated sprites supply a large, consistent vocabulary of ways to render it.

## M2 Boundaries

M2 uses the current Kilteevan PNG atom kit for proof and adds the topology-aware generator/validation layer. It does not yet batch-generate new raster sprites, build atlas packing, or persist the generated scene pack into the live mod index. Those become safer once the constraint model can reject impossible placements.
