Use case: historical-scene
Asset type: isomorphic game environment background plate, native 16:9 desktop, no UI

Input images and roles:
Image 1: cleaned top-down control plate for this exact map crop; primary topology/control image for this second stage.
Image 2: original historic map crop; primary source evidence if the control plate and prompt disagree.
Image 3: original illustrated parish notebook scene; STYLE REFERENCE ONLY. Use it only for art direction: rough sepia pen line, hand-inked watercolor, mottled parchment grain, muddy roads, slate roofs, limewashed walls, irregular stone walls, dense field-sketch hatching, uneven watercolor washes, cool overcast shadows, and literary notebook mood. Do not copy its UI, people, church, chapel, shop, bridge, river, road labels, signs, names, smoke, animals, carts, composition, or any specific landmark.
Image 4: tiny style/material swatch only; use only for texture/color/line/material treatment.
Image 5: tiny style/material swatch only; use only for texture/color/line/material treatment.

Inserted map-reader notes:
# Data-Derived Map Reader Notes

## Scope
These notes are derived only from the attached historic map crop using the generic rubric. The image top is treated as north, printed labels and paper texture are ignored, and uncertain marks are described as probabilities rather than fixed facts.

## Orientation And Major Corridors
- West and northwest curving corridor: broad pale lane or road entering from the west edge, bending through the northwest quadrant, and continuing toward the north edge. Parallel/open edges and roadside planting make this a high-confidence road or lane.
- South-west to center-left corridor: broad pale lane or road running along the lower-left edge toward the central building group. It is lined with frequent tree symbols and has enough width to read as a physical route, high confidence.
- Northeast to center-right corridor: broad pale diagonal lane or road descending from the upper center/right toward the central-right area, with parallel/open edges and tree symbols along portions of it. High confidence as a road or lane.
- Short center-left approach: a faint curving connection from the lower-left lane toward the central yard/building frontage may be an entrance track or yard approach. Medium confidence because it is partly obscured by trees and yard marks.
- Single thin field and enclosure lines in the north, northeast, east, and southeast are more likely walls, hedges, ditches, or plot boundaries than roads. Medium confidence for boundary function, low confidence for exact material.
- Dotted or pecked lines that cut across open ground, especially the north-south dotted line near the lower center, are ambiguous and may be administrative or survey boundaries. Low confidence as physical features; they should not be treated as continuous roads, hedges, walls, fences, ditches, or planted rows without separate corroborating marks.

## Building Inventory
B1: West edge, upper-left. Partial narrow horizontal rectangle. Dark hatched rectangle clipped by the crop edge. Building fragment or small outbuilding, medium-low confidence. Only the visible portion should be represented; footprint continues beyond crop or is incomplete.
B2: West edge, center-left beside the curving road. Partial angled or compact roofed footprint. Small roadside building or outbuilding, medium-low confidence. Keep small and partial.
B3: Northwest quadrant beside the curving lane. Irregular angled/L-like footprint. House, farm building, or roadside outbuilding, medium confidence.
B4: Just north of the central planted enclosure. Small narrow horizontal rectangle. Shed, small barn, privy, or minor outbuilding, medium confidence. Detached and modest.
B5: Center-left at the south edge of the planted enclosure. Compact irregular block near the yard entrance. Outbuilding or ancillary farm structure, medium confidence.
B6: Center-south, along the main yard/frontage. Long horizontal hatched rectangle. Probable primary house or main farm building, high confidence.
B7: Center-south/east, just east of B6. Shorter horizontal hatched rectangle. Barn, stable, byre, or secondary domestic/farm building, high confidence. Distinct from B6.
B8: Center-right beside the diagonal lane. Tall narrow north-south rectangle. Barn, stable, cart shed, or larger outbuilding, high confidence. Oriented north-south.

## Enclosures, Planting, And Boundaries
- Central planted enclosure: a rectangular enclosed area in the center-left/center, subdivided into regular beds and filled with repeated planting marks. High confidence as a garden, orchard, nursery, or formal planted yard.
- Central yard: open pale space south and east of the planted enclosure, bounded by buildings, trees, and lane edges. Medium-high confidence as a working yard or forecourt.
- Tree clusters around the lower-left lane: many repeated round and conifer-like symbols line both sides of the road. High confidence as roadside trees or mixed hedgerow planting; individual symbols can be read as trees rather than a continuous wall.
- Tree symbols along the northeast/center-right lane: scattered tree marks follow the broad diagonal corridor. High confidence for roadside or boundary planting, medium confidence for whether they form a formal avenue.
- Southeast and lower-center field boundary planting: scattered tree symbols sit along or near thin boundary lines. Medium confidence as hedgerow or field-edge trees, but the line itself may be a hedge, wall, ditch, or survey boundary depending on local context.
- North and northeast field polygons: thin angular lines define large enclosed parcels. Medium confidence as field or estate boundaries; low confidence for material.
- Dotted/pecked north-south line near the lower center: likely administrative, survey, parcel, or otherwise ambiguous unless separately supported. It cuts through open ground and should not be rendered as a continuous physical feature.
- Other isolated dotted or pecked marks should be treated cautiously. Where dots coincide with a broad lane edge and roadside trees, they may support a road-edge or planted boundary reading; where they stand alone, they remain non-physical or ambiguous.

## Explicit Negative Evidence
No church evidence. No shop evidence. No water evidence. No bridge evidence. Printed labels and large map text are not in-world objects. No UI or modern interface marks are visible as in-world features. No smoke, fire, or active industrial plume evidence is visible. Administrative or survey boundary rendering should be avoided for dotted/pecked/dashed lines that lack independent physical cues.

Primary request:
Convert the cleaned top-down control plate into one finished illustrated background plate for a historical isomorphic game. Preserve the roads, yards, planted enclosures, building footprints, tree clusters, and physical boundaries from the top-down plate while lifting them into a 3/4 orthographic isomorphic view. This is the base environment layer only.

Camera and geometry:
Fixed 3/4 orthographic isomorphic game camera. Strongly enforce a consistent game-board perspective, not a drone photograph and not a steep survey view. Keep all walkable surfaces on one stable ground plane. Show rooftops plus readable vertical facades, doors, thresholds, yards, gates, and wall faces. No horizon, no sky, no vanishing point, no cinematic perspective. Keep north up: source-map top and top-down control top remain final-image top; east is right, south is bottom, west is left. Do not rotate the ground plan into a prettier diagonal composition.

Scale and sprite readiness:
Frame the local site at a playable zoom level for small character sprites, matching Cycle M's useful zoom level. Buildings should be readable but not close-up. Roads, yards, garden beds, gates, and building entrances must remain wide and clear enough for characters to move around.

Map and control fidelity:
Use Image 1 as the plan to lift into illustrated space. Preserve route continuity, boundary geometry, road count, yard openness, planted enclosure positions, building footprint sizes, building orientations, and tree-cluster positions. Image 2 and the map-reader notes are source evidence. Style must never override topology. Do not add new roads, paths, field walls, gardens, rivers, bridges, buildings, yards, wells, churches, graveyards, shops, people, animals, carts, signs, labels, smoke, or UI.

Administrative/survey boundary handling:
Do not reintroduce any dotted, pecked, dashed, dot-chain, administrative, survey, townland, parish, barony, county, estate, or parcel boundary that the top-down control omitted or the notes mark as non-physical/ambiguous. Those marks are not terrain. Do not render them as hedges, bushes, walls, fences, ditches, roads, paths, crop rows, ridges, tree rows, shadows, or decorative texture.

Architecture:
Rural early-19th-century Irish vernacular where the map supports buildings: limewashed stone walls, gray slate or dark thatch where plausible, low simple rectangular forms, sheds/byres/barns as modest service structures. Many period huts had no chimneys; chimneys are optional and should be rare. No freestanding random chimneys, no chimneys embedded in walls, no chimneys stuck in garden walls or field walls, no decorative roof stacks unless coherent and attached to a substantial rendered building.

Style target, high priority:
Make the final rendering visibly belong to the same art family as Image 3, the original illustrated parish notebook. It should feel hand-drawn, literary, historical, and slightly rough. Use uneven brown-black/sepia ink contours, scratchy field-sketch hatching, dark broken roof hatching, dirty limewash stains, dry-brush stone texture, muddy ochre road scumbling, tiny irregular grass strokes, mottled olive watercolor washes, cool gray-blue shadows, softened paper grain, and imperfect hand-painted edges. Let the terrain have local value variation, ink noise, stains, and watercolor blooms. Roads should be muddy and textured, not smooth beige ribbons. Fields should not be flat uniform green; they should have blotchy washes, sparse grass marks, and subtle paper tooth. Stone walls should be irregular, broken, and hand-inked, not clean chains of identical stones. Roofs should be darker and sketchier, closer to the notebook's slate/thatch hatching than clean strategy-game roof tiles.

Style anti-regression:
Do not make this look like a clean vector map, polished mobile strategy-game terrain, miniature board-game diorama, toy model, flat tile set, smooth 3D render, or uniformly green simulation plate. Do not over-clean the watercolor. Do not make walls, fields, gardens, or roof tiles perfectly regular. Do not make the scene prettier by adding unsupported landmarks or decorative clutter.

Walkability/topology:
Roads, lanes, yards, gates, entrances, and thresholds must remain continuous and unobstructed. Where a route crosses a physical boundary, show a gate/opening. Add only surface texture and edge roughness; do not add props that block walkable space.

Native 16:9 framing:
Generate the plate natively as 16:9 from the available crop/control context. No post-generation side padding, mirrored margins, blurred edge extension, cloned fields, decorative borders, or synthetic edge fill.

Hard constraints:
No UI, no labels, no visible text, no signs, no map pins, no copied survey numbers, no people, no animals, no carts, no smoke, no fog, no invented watercourses, no bridges, no churches, no graveyards, no shops, no decorative chimneys, no freestanding random chimneys, no chimneys embedded in walls, no copied style-reference objects.
