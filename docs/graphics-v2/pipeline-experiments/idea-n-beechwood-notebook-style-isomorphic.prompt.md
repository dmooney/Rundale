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
These notes are derived only from the attached historic map crop using the generic rubric. The image top is treated as north, printed labels/large letters/survey numbers/paper texture are ignored as in-world objects, and uncertain marks are described with confidence rather than converted into hard constraints.

## Orientation And Major Corridors
- Main diagonal road or lane, high confidence: A broad pale corridor with parallel edges enters from the northwest edge, runs diagonally through the center-left, passes directly beside the main building group, and continues toward the south-southeast/lower-right edge. Its width and paired edges support reading it as a road/lane rather than a thin boundary.
- Roadside yard frontage, medium-high confidence: The building group sits tightly against the main corridor around the central and south-central crop, suggesting a road-facing yard or service edge. Some dark structures directly touch or nearly touch the lane edge.
- Thin single-line field or plot boundaries, medium confidence: Several single thin lines curve or angle through the southwest, south, and east portions of the crop. By rubric these are more likely walls, hedges, ditches, or plot boundaries than paths; the exact material is uncertain.
- Prominent dotted/pecked line in the northeast and east, high confidence as administrative/survey or ambiguous non-physical boundary: A dot-chain line curves from the north edge down the eastern side, cutting across open parcel areas without corroborating tree symbols, road edges, wall hatching, ditch marks, or gate/yard relationships. It should not be rendered as a continuous hedge, wall, fence, road, path, ditch, or tree row.
- Faint dashed/pecked line near center-left to south-center, medium confidence as ambiguous/non-physical: A faint broken line appears to cross open ground near the building group and lower central crop. Because it lacks independent physical cues, it should be treated as administrative/survey/ambiguous rather than drawn as an in-world continuous feature.

## Building Inventory
B1: Center-left, immediately east/southeast of the main diagonal lane. Dark hatched rectilinear block, roughly L-shaped or range-like, with a pale inner/open area adjacent to the road. Primary house, farmhouse, or main roadside building. Medium-high confidence. Render as the largest roofed building in the group, aligned with the lane; allow a small attached/open yard or court where the pale gap appears.
B2: South-central, just below the large central enclosed planted rectangle and close to the lane. Small dark rectangular block, detached or lightly attached to the enclosure/yard edge. Shed, stable, byre, small barn, or service outbuilding. Medium confidence. Render as a small auxiliary roofed structure, lower and simpler than B1.
B3: Lower center-left/south-central, on the opposite side of the lane from the large enclosed planted area. Compact cluster of dark hatched rectilinear pieces forming an irregular small compound. Farm outbuildings, barn/stable/byre range, or service yard. Medium confidence. Render as a modest outbuilding cluster rather than a single grand house; individual pieces may be connected or very close together.
B4: Center-right/southeast of the large enclosed planted rectangle, beside or just off the lane. Very small dark rectangular block. Minor shed, privy, small store, or other outbuilding. Low-medium confidence. Render only as a small secondary structure if needed; keep it subordinate and uncertain.

## Enclosures, Planting, And Boundaries
- Large central enclosed planted area, high confidence: A large pale rectangle occupies the center to center-right, outlined by a thin boundary and filled with regular repeated internal marks. It reads more like a garden, planted yard, nursery, orchard, or formal ground than a roofed building.
- Small regular northern enclosures, medium confidence: Two small outlined rectangles north of B1 and northwest of the large central planted rectangle may be garden beds, yard compartments, small pens, or non-roofed enclosures. They are not strongly marked as roofed buildings.
- Northwest and center-left tree/scrub mass, high confidence: Dense clusters of round and small tree symbols occupy much of the northwest quadrant and center-left west of the road, suggesting woodland edge, scrub, orchard-like planting, or thick hedgerow planting.
- Southern small tree/scrub clusters, medium-high confidence: Smaller clusters of tree symbols appear around the lower central building group and near the southeast edge, suggesting planted edges, orchard fragments, scrub, or hedgerow vegetation.
- Open field or parcel areas, medium confidence: Pale open areas with thin boundary lines occupy the southwest, east, and northeast portions. The printed stipple/paper texture is not treated as vegetation; only explicit tree or planting symbols are rendered as plantings.
- Curving southwest/south boundary, medium confidence: A thin curving line along the southwest and lower portions likely marks a plot, field, wall, hedge, or ditch edge. Material is uncertain and should remain visually modest.
- Eastern dotted/pecked boundary, high confidence non-physical/ambiguous: The curving dot-chain boundary on the east lacks physical support and should be omitted as a continuous terrain object. It may be useful only as invisible source-map context.
- Faint broken center-left boundary, medium confidence non-physical/ambiguous: The faint dashed/pecked line near the lower center-left has insufficient physical cues. If represented at all, it should be treated as uncertain survey information, not as a hedge, path, ditch, or fence.

## Explicit Negative Evidence
No church, churchyard, cross, graveyard enclosure, ecclesiastical footprint, or church label evidence appears in the crop. No shop evidence. No clear watercourse, pond, bridge, marsh/water edge, or water hachure appears. Printed labels, large letters, parcel/survey numbers, and paper texture are not in-world objects. No in-world UI, modern overlay object, smoke plume, vehicles, people, or animals are evidenced by the crop. Dotted/pecked and faint dashed lines should not be converted into bushes, hedges, walls, fences, paths, roads, ditches, or tree rows.

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
