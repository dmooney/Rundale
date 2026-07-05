Use case: historical-scene
Asset type: bounded edit/refinement of an existing low 3/4 orthographic isomorphic game environment background plate, native 16:9 desktop, no UI

Primary request:
Create one stricter bounded visual refinement. Image 1 is the topology authority. Image 2 is a prior notebook-style refinement from the same topology target. Preserve Image 1's topology more strictly than Image 2 did, while borrowing Image 2's improved ink/watercolor atmosphere where it does not change layout.

This is a repaint/cleanup pass, not a new render. Do not reinterpret the map. Do not generate a new location. Do not use hidden context, route graphs, generated route graphs, or hand-authored place-specific notes.

Input images:
Image 1: original topology-preserving target and layout authority. Preserve its north-up local layout, road geometry, building count, roofed structures, building footprints, garden/orchard placement, tree/field masses, walls/hedges, yard spaces, gate openings, frame crop, and 16:9 composition.
Image 2: prior bounded refinement. Use only as a style/cleanup reference: rougher notebook ink, scumbled roads, mottled fields, no carts/barrels, no smoke/chimneys, clearer doors. Do not copy any topology loss from Image 2.
Image 3: original illustrated parish notebook sample. Style and low playable camera feel only: sepia ink, watercolor wash, paper grain, muddy roads, readable facades, dense hand detail, rough vegetation, and notebook atmosphere. Do not copy UI, labels, people, church, graveyard, river, bridge, shop, signposts, carts, animals, named places, chimneys, smoke, composition, or landmarks.
Image 4: cleaned slate-roof single-house style reference. Use only for limewashed rural facade, timber plank door, threshold, hand ink, slate roof texture, building scale, and no-chimney discipline.
Image 5: cleaned thatched/no-chimney single-house style reference. Use only for possible thatch, rough eaves, timber plank door, threshold, and no-chimney discipline.
Image 6: tree/field watercolor style reference. Use only for soft open fields, uneven grass, hedges, scrub, field texture, and watercolor vegetation.

Topology invariants:
Keep Image 1's road centerlines, road widths, road exits, lane/yard continuity, gate locations, building locations, building footprints, number of visible roofed structures, garden/orchard extent, tree mass positions, field extents, and overall camera/crop. Do not add, remove, widen, narrow, straighten, curve, or reroute roads. Do not add footpaths. Do not move roads, buildings, garden edges, wall lines, tree belts, field edges, or gate openings.
Every roofed structure or roof-like built structure visible in Image 1 must remain a distinct roofed or built structure in the output unless it is unmistakably a movable cart, barrel, tub, table, crate, or freestanding clutter. Do not flatten a roofed outbuilding into a wall, gate, fence, hedge, plant clump, shadow, or garden mark.
Remove only clearly movable nonessential props: carts, barrels, tubs, crates, table-like objects, loose clutter, freestanding objects not attached to a building. When in doubt, preserve the object as a small outbuilding/roofed structure rather than deleting it.

Allowed changes:

- Increase the original parish notebook feel: rougher sepia/brown-black ink, looser watercolor washes, paper tooth, scratchy field hatching, dirty limewash, muddy scumbled roads, irregular grass strokes, mottled olive fields, dry-brush stone texture, and imperfect hand-painted edges.
- Make existing facades, doors, thresholds, walls, gates, muddy road edges, garden beds, and vegetation slightly more readable.
- Remove or paint out only clearly movable props and clutter.
- Remove chimney-like roof/wall marks, roof nubs, vents, pipes, smoke holes, and smoke.
- Add or clarify plank doors only on existing visible person-sized building openings. Do not create new buildings or new entrances where no facade/opening exists.

Camera:
Preserve Image 1's orthographic/isomorphic north-up ground plan and crop. You may make the image feel slightly more like the low playable notebook sample by increasing facade readability and side-wall texture, but do not rotate, zoom, recompose, or redraw the ground plan. No horizon, no sky, no vanishing point.

Doors on openings:
Every visible walkable house, cottage, barn, byre, shed, or outbuilding that shows any facade must have one readable human-usable timber plank door on a visible facade, plus a small threshold connected to a yard or road. If Image 1 contains or you paint any person-sized dark vertical opening, doorway, entry gap, shed mouth, barn mouth, or black rectangular hole in a wall, put a visible brown or weathered gray-brown timber plank door directly on that opening, fitted inside the opening, with vertical plank marks or a half-open plank slab. Do not leave empty black door holes. Do not imply a doorway with shadow only. Do not place a door beside an opening while the opening remains empty.

Roof rule:
No visible chimneys anywhere. No random chimneys. No chimney-like stacks, roof nubs, vents, pipes, capstones, wall chimneys, isolated vertical posts, roof pegs, ridge boxes, smoke holes, black puffs, or protrusions embedded in roofs or walls. No visible smoke. Slate roofs must be continuous rough slate planes with inked tile texture only. Thatched roofs must be continuous rough thatch with no roof holes, no smoke holes, no protruding stacks, and no vertical objects.

Garden and field material:
Preserve Image 1's garden/orchard and field geometry. Make the texture more organic and hand-painted, but do not change the plot geometry. Garden bed lines should read as planting/soil texture, not as new roads or extra walls. Do not harden garden internals into extra walls. Open fields should remain open, with mottled watercolor and sparse scrub instead of continuous new boundary lines.

Hard negative constraints:
No UI, labels, visible text, signs, map pins, people, animals, carts, barrels, tubs, crates, smoke, fog, weather effects, watercourses, rivers, streams, ponds, bridges, churches, chapels, graveyards, shops, market squares, wells, decorative chimneys, freestanding chimneys, chimneys embedded in walls, roof nubs, copied style-reference objects, extra roads, extra footpaths, scenic recomposition, added buildings, moved buildings, deleted roofed structures, or roofed structures flattened into gates/walls/hedges.
No black person-sized door holes without actual plank doors fitted into the holes.

Output:
One clean 16:9 illustrated low 3/4 isomorphic background plate, no UI. Success means: Image 1 topology remains recognizable and stricter than Image 2; every roofed/built structure from Image 1 remains distinct unless clearly movable clutter; art moves closer to the original parish notebook look; every visible person-sized opening has a visible plank door fitted into it; nonessential movable props are removed; no chimneys/smoke/roof nubs; no semantic leakage.
