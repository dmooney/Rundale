Use case: historical-scene
Asset type: game environment concept art, 16:9 desktop background plate, no UI

Primary request:
Generate a fresh direct-control background plate for a historical isometric game from the supplied tighter playable map crop and matching cleaned control crop. Preserve the map-derived topology and north-up orientation while moving toward the original illustrated parish notebook look: loose hand ink, watercolor washes, readable facades, muddy roads, soft open fields, no UI, no labels, no smoke.

This is a one-shot/direct-control experiment. Do not use any previous generated plate as an edit target, style target, layout reference, or composition reference. Do not rely on hand-authored location-specific road, building, river, or landmark notes. Infer the place from the supplied map/control images only, using the generic rules below.

Input images and authority order:
Image 1: tight original historic Ordnance Survey map crop. Primary source evidence for roads/lanes, dark roof/building marks, enclosed yards, planted areas, tree/scrub symbols, field/parcel divisions, and geographic orientation. Top of this image is north.
Image 2: tight cleaned no-admin map crop. Physical-linework control. Dotted/pecked administrative or survey dot chains have been suppressed. Soft grey erased seams, pale diagonal smears, and faint scars in this image are deletion artifacts, not terrain.
Image 3: oblique warped tight cleaned control. Camera/pitch cue only for strict 3/4 orthographic/isomorphic perspective. Do not copy blank borders, paper texture, warp artifacts, text fragments, or erased diagonal scars.
Image 4: original illustrated parish notebook sample. Style only: loose ink/watercolor density, rough muddy roads, readable facades, low playable camera feeling, varied brushwork, and paper atmosphere. Do not copy UI, labels, people, church, graveyard, river, bridge, shop, signposts, carts, animals, named places, roof chimneys, smoke, or composition.
Image 5: cleaned slate-roof single-house style reference. Use for limewashed rural facade, dark timber doorway, threshold, hand ink, slate roof texture, no-chimney roof discipline, and building scale.
Image 6: cleaned thatched/no-chimney single-house style reference. Use for possible thatch, rough eaves, dark timber doorway, threshold, and no-chimney discipline.
Image 7: tree/field watercolor style reference. Use for soft open fields, uneven grass, hedges, scrub, field texture, and watercolor vegetation only. Do not copy animals or non-map content.

Composition lock:
The tighter map crop is already the intended local playable area. Do not zoom back out. Do not add off-crop context. Do not invent a balanced scenic village or picturesque crossroads to make the image prettier. Preserve the awkwardness of the crop if roads, boundaries, trees, or buildings enter/exit at the edges. Roads and lanes should continue naturally off-frame instead of being rearranged into a centered scenic intersection.
Keep the relative placement, spacing, and grouping of dark roof marks, broad pale road corridors, planted/garden texture, and tree/scrub symbols from Images 1-2. Do not add extra cottages, sheds, compounds, gates, or walls to fill empty corners.

Conflict rules:
The map/control images outrank all style references for content and topology.
The single-building style crops outrank the full notebook sample for roofs, doors, thresholds, and chimney discipline.
If the full notebook sample suggests a chimney, smoke, church, bridge, people, cart, sign, water, UI element, or extra building type, ignore it completely.
If a feature is not visible in the map/control images, omit it.

Generic map interpretation policy:
Ignore printed labels, partial place-name text, large letters, numerals, survey text, stains, paper dots, and typography.
Broad pale corridors are roads or lanes.
Dark filled or hatched rectangles attached to yards/lanes are buildings or outbuildings.
Dense tree/scrub symbols are vegetation masses, not buildings.
Regular internal garden strokes are planting/bed texture, not walls.
Thin solid lines may be plot boundaries, hedge lines, low banks, ditches, overgrown walls, or survey parcel edges; they are not automatically roads or stone walls.
Dotted, pecked, dashed, or dot-chain lines are likely administrative/survey boundaries unless corroborated by physical symbols; render no physical trace for them.

Open-field boundary rule, highest priority:
Open fields should remain visually open. Ordinary thin field/parcel lines are uncertain cartographic boundaries, not automatic walls. Render most ordinary thin lines as no visible feature, faint grass color shifts, shallow drainage dips, broken hedge clumps, low overgrown banks, scattered scrub, or subtle field texture changes. They should be easy to overlook at first glance.
Only the clearest domestic yards, building compounds, and planted garden/orchard/nursery enclosures may receive visible boundary treatment. Even there, prefer mixed, low, broken, irregular boundaries: short stone wall fragments, gaps, hedges, earth banks, rough gate openings, or overgrown wall remnants.
Do not create a connected stone-wall network across open fields. Do not outline every field. Do not run stone walls along both sides of every road. Do not make a chessboard of walls. Do not trace long straight walls through open grass unless the map shows a strong enclosed yard/garden/compound.

Boundary hierarchy:
Tier 0: suppressed dotted/pecked/admin/survey linework from Image 2. Render nothing. No wall, hedge, fence, ditch, road, footpath, crop row, tree row, ridge, shadow, seam, or vegetation trace.
Tier 1: broad pale road corridors. Render as muddy unfenced rural lanes/roads with soft grass shoulders, wheel ruts, stones, puddle marks, and uneven edges. Roads can have occasional short wall or hedge fragments near yards/gates, but not continuous wall borders.
Tier 2: immediate domestic yards and building compounds. Use readable but broken boundaries only where needed to define yards: short low wall fragments, hedge/bank segments, open gates, gaps, and worn thresholds.
Tier 3: planted garden/orchard/nursery enclosures. Outer edges may be clearer than field lines, but should still be mixed low wall/hedge/bank with breaks and a gate. Internal bed lines are plants and soil texture, not walls.
Tier 4: ordinary open-field parcel lines. Mostly invisible or soft vegetation/ditch/grass texture. No continuous stone wall treatment.

Perspective and composition:
Keep north-up: top of source map remains toward the top/north of final plate; bottom remains south. Do not rotate the ground plan.
Use strict 3/4 orthographic/isomorphic game perspective. Not top-down, not survey-board, not aerial landscape. Building facades, doors, thresholds, gates, and road edges should be readable for 2D character navigation.
Use a wide 16:9 plate at playable scale. The final image should correspond to the supplied tight crop, not the larger area outside it.
Roads must connect plausibly and continue naturally off-frame. Do not add decorative footpaths. Do not dead-end roads at erased survey lines. Do not route paths through buildings or closed boundaries without gates/openings.

Buildings:
Render only buildings supported by dark roof/building marks in the map/control images. Use humble early-19th-century rural Irish vernacular: low limewashed stone or rough stone, patched plaster, thatch where plausible, slate where plausible, low eaves, small windows, uneven walls, muddy thresholds.
Every visible walkable house/cottage/outbuilding must have a readable human-usable dark timber doorway on a visible facade, with a small threshold connected to yard or road. This includes small and edge buildings if they read as enterable. Do not mistake a window shadow for a door.
Buildings must not sit in roads. Buildings must respect map-derived roof footprints and road frontage. Do not invent extra buildings where the crop only shows labels, thin lines, trees, or paper texture.

Absolute roof rule:
No visible chimneys anywhere. No random chimneys. No chimney-like stacks, roof nubs, vents, pipes, capstones, wall chimneys, isolated vertical posts, roof pegs, ridge boxes, smoke holes, black puffs, or protrusions embedded in roofs or walls. No visible smoke.
If the model is uncertain whether to add a chimney, do not add one.
Slate roofs must be continuous rough slate planes with inked tile texture only.
Thatched roofs must be continuous rough thatch with no roof holes, no smoke holes, no protruding stacks, and no vertical objects.
Roof texture may be mottled, patched, mossy, and hand-painted, but it must not form isolated vertical marks.

Hard negative constraints:
No physical trace along suppressed dotted/pecked/admin boundaries from Image 2.
No water, rivers, streams, ponds, bridges, church, chapel, graveyard, shopfront, market square, UI, labels, map text, people, animals, carts, barrels, smoke, fog, or weather effects.
No picturesque/generic crossroads composition. No invented scenic balancing buildings. No extra roads or footpaths.
Do not copy semantic content from the full notebook sample; it is style only.
No overly regular roof grids, garden grids, perfect walls, continuous field outlines, fantasy styling, photorealism, 3D rendering, clean vector map, mobile-game tile look, toy miniature look, or modern architecture.

Style:
Original illustrated parish notebook environment art: sepia/dark ink outlines, sketchy crosshatching, loose watercolor washes, visible paper grain, muted greens/ochres/stone greys, muddy road whites and browns, rough vegetation blobs, uneven brushwork, imperfect hand-painted edges, lived-in rural texture. Rich and inspectable at game scale, but a clean static background plate with no UI or animated effects.

Output:
One clean 16:9 illustrated isomorphic background plate, no UI. Success means: tighter-crop direct-control topology fidelity, north-up perspective, no scenic-composition drift, open-field softness, no restored deleted-admin boundary, readable doors on all visible walkable buildings, no chimneys/smoke/roof nubs, and stronger original-notebook art feel.
