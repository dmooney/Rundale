Use case: historical-scene
Asset type: low 3/4 orthographic isomorphic game environment background plate, native 16:9 desktop, no UI

Primary request:
Convert the supplied top-down cleaned control plate into one finished illustrated background plate for a historical isomorphic game. Preserve the map-derived road, yard, building, planted-enclosure, tree, and open-field topology while lifting it into a low 3/4 orthographic/isomorphic view with the original illustrated parish notebook look. This is a base environment plate only: no UI, no text, no characters, no smoke layer.

Execution context:
Use only the images attached with this request and this text prompt. Do not use prior conversation context, prior generated plates other than Image 1, hidden location notes, or hand-authored location interpretations. The pipeline must remain generic and data-driven.

Input images and authority order:
Image 1: generated top-down cleaned control plate for this same map crop. Use it as a broad topology, terrain, and material organization control, but treat it as fallible because generated controls can accidentally materialize erased admin/survey seams or over-regularize field lines.
Image 2: original historic Ordnance Survey-style map crop. Highest authority for source evidence, orientation, broad road corridors, dark roof/building marks, planting symbols, and whether a feature is actually supported by the map.
Image 3: cleaned no-admin map crop. Highest veto authority for suppressed dotted/pecked/dashed administrative/survey linework. Soft gray erased seams, pale diagonal smears, and faint scars are deletion artifacts, not terrain.
Image 4: deterministic oblique warp of the cleaned no-admin crop. Camera/pitch cue only. It shows how a north-up map plane compresses under a low 3/4 camera. Do not copy its beige margins, strip composition, scan texture, text fragments, line artifacts, erased seams, or exact crop.
Image 5: original illustrated parish notebook sample. Style and low playable camera feel only: sepia ink, watercolor wash, paper grain, muddy road texture, readable facades, dense hand detail. Do not copy UI, labels, people, church, graveyard, river, bridge, shop, signposts, carts, animals, named places, chimneys, smoke, composition, or landmarks.
Image 6: cleaned slate-roof single-house style reference. Use for limewashed rural facade, dark timber doorway, threshold, hand ink, slate roof texture, building scale, and no-chimney discipline.
Image 7: cleaned thatched/no-chimney single-house style reference. Use for possible thatch, rough eaves, dark timber doorway, threshold, and no-chimney discipline.
Image 8: tree/field watercolor style reference. Use for soft open fields, uneven grass, hedges, scrub, field texture, and watercolor vegetation only.

Absolute camera target:
Strict low 3/4 orthographic/isomorphic game camera, around 30-35 degrees above the ground plane. This is not top-down and not a high survey plate. Show rooftops plus unmistakable vertical facades, doors, thresholds, wall side faces, gate posts, low boundary side faces, and dark lower tree masses. Keep all walkable surfaces on one stable ground plane. Parallel map edges remain parallel; no vanishing point.
Keep north up: source-map top and top-down-control top remain toward the top/north of the final image; east is right, south is bottom, west is left. The camera is south of the scene looking north. The ground plane may compress toward the top, but do not rotate the ground plan into a prettier diagonal composition. No horizon, no sky, no cinematic perspective, no fisheye, no drone/aerial-survey feel.

Composition lock:
The tight 16:9 map crop is already the intended local playable area. Do not zoom back out. Do not add off-crop context. Do not invent a balanced scenic village or picturesque crossroads. Preserve the awkward local crop if roads, boundaries, trees, gardens, or buildings enter/exit the frame edges. Roads and lanes should continue naturally off-frame instead of being rearranged into a centered scenic intersection.
Use Image 1 to understand the broad local layout, but use Images 2-3 to veto any line or terrain feature that Image 1 appears to have invented.

Generic map/control conflict rules:
The original map crop and cleaned no-admin crop outrank the generated top-down control for feature existence.
The generated top-down control may guide painted terrain and approximate relative placement only where it agrees with Images 2-3.
If Image 1 contains a continuous hedge, wall, field line, seam, road, path, crop row, or vegetation chain that is absent from Images 2-3 or aligns with a cleaned/suppressed administrative/survey scar, omit it from the final plate.
If Images 2-3 show only a pale erased seam, soft gray deletion scar, diagonal smear, dot-chain removal mark, label remnant, survey text, or paper texture, render open field or ordinary grass wash there.
Do not preserve generated control mistakes just because they are visually coherent.

Administrative/survey boundary rule:
Dotted, pecked, dashed, or dot-chain linework on historic maps can mark non-physical administrative/survey divisions: townland, parish, barony, county, estate, parcel, or survey boundaries. These are not terrain unless corroborated by physical evidence. Unsupported admin/survey boundaries must leave no physical trace. Do not render them as hedges, bushes, walls, fences, ditches, roads, paths, crop rows, ridges, tree rows, shadows, seams, color changes, or decorative texture.
Only draw a boundary as physical when supported by independent map evidence: paired road edges, tree/hedge symbols riding the line, wall/ditch hatching, enclosure-edge continuity, gate/yard relationships, or a clearly domestic/garden compound edge. If uncertain, omit the physical trace.

Open-field boundary hierarchy:
Open fields must stay visually open. Ordinary thin parcel or field lines are uncertain cartographic boundaries, not automatic walls. Render most ordinary thin lines as no visible feature, faint grass color shifts, shallow drainage dips, sparse broken hedge clumps, low overgrown banks, or subtle field texture changes. They should be easy to overlook at first glance.
Only clear domestic yards, building compounds, and planted garden/orchard/nursery enclosures may receive visible boundary treatment. Even there, prefer mixed low broken irregular boundaries: short stone wall fragments, gaps, hedges, earth banks, rough gate openings, overgrown wall remnants.
Do not create a connected stone-wall network across open fields. Do not outline every field. Do not run continuous walls along both sides of every road. Do not make a chessboard of walls. Do not trace long straight walls through open grass unless the original map shows a strong physical yard/garden/compound edge.

Roads, paths, and walkability:
Broad pale corridors in Images 2-3 are muddy rural roads or lanes. Keep them broad, continuous, unfenced unless map-supported, and clear for character movement. Use soft grass shoulders, wheel ruts, stones, puddle-like damp marks only in roads, and uneven worn edges.
Do not invent decorative footpaths. Do not convert thin parcel lines into paths. Do not route paths around erased admin seams. Do not dead-end a road against a seam or line artifact. Where a route crosses a physical boundary, show a real gap, gate, opening, or worn threshold. Never place buildings, walls, trees, garden beds, props, or vegetation masses in road centers, gate openings, entrances, or yard centers.

Buildings:
Render only buildings supported by dark roof/building marks in Images 2-3, with approximate footprint size, separation, orientation, and road/yard relationships preserved from the map and top-down control.
Use humble early-19th-century rural Irish vernacular: low limewashed stone or rough stone walls, patched plaster, slate or thatch where plausible, low eaves, small dark windows, uneven walls, muddy thresholds, service outbuildings as modest sheds/byres/barns/stables.
Every visible walkable house, cottage, barn, byre, shed, or outbuilding that shows any facade must have one readable human-usable dark timber doorway on a visible facade, plus a small threshold connected to a yard or road. This includes small edge buildings if they read as enterable. A black window, shadow patch, roof edge, or wall stain does not count as a door. If a building is visible enough to be walkable, its entrance must be readable.
Buildings must not sit in roads. Buildings must not block lanes, yard centers, gates, or thresholds. Do not invent extra cottages, sheds, shops, barns, compounds, or decorative buildings to fill empty corners.

Absolute roof rule:
No visible chimneys anywhere. No random chimneys. No chimney-like stacks, roof nubs, vents, pipes, capstones, wall chimneys, isolated vertical posts, roof pegs, ridge boxes, smoke holes, black puffs, or protrusions embedded in roofs or walls. No visible smoke.
If uncertain whether to add a chimney, do not add one.
Slate roofs must be continuous rough slate planes with inked tile texture only.
Thatched roofs must be continuous rough thatch with no roof holes, no smoke holes, no protruding stacks, and no vertical objects.
Roof texture may be mottled, patched, mossy, and hand-painted, but it must not form isolated vertical marks.

Planted areas and vegetation:
Mapped garden/orchard/nursery/internal planting should become planted beds, low shrubs, orchard rows, or garden texture with handmade irregularity. Internal bed lines are soil and planting texture, not stone walls and not extra paths unless clearly broad and walkable.
Mapped tree/scrub symbols become tree canopies, dark lower masses, trunks where visible, scrub clumps, and hedgerow fragments where supported. Do not turn every tree symbol chain into a continuous wall. Do not fill roads with trees.

Notebook art target:
The final plate should feel like the original illustrated parish notebook environment after the UI has been removed. Use uneven sepia/brown-black ink contours, scratchy field hatching, broken roof hatching, dirty stained limewash, dry-brush stone texture, muddy ochre road scumbling, tiny irregular grass strokes, mottled olive watercolor fields, cool gray-blue shadows, softened paper grain, watercolor blooms, and imperfect hand-painted edges. Increase local value variation and hand-drawn texture. Roads should be rutted and hand-scrubbed rather than smooth pale ribbons. Fields should be blotchy and varied rather than uniform green. Walls and hedges should be rough, broken, low, irregular, and overgrown where present.

Hard negative constraints:
No UI, labels, visible text, signs, map pins, copied survey numbers, people, animals, carts, barrels, smoke, fog, weather effects, invented watercourses, rivers, streams, ponds, bridges, churches, chapels, graveyards, shops, market squares, wells, decorative chimneys, freestanding chimneys, chimneys embedded in walls, roof nubs, copied style-reference objects, or semantic content from the notebook sample.
No physical trace along suppressed dotted/pecked/admin/survey linework from Image 3.
No picturesque/generic crossroads composition. No invented scenic balancing buildings. No extra roads or footpaths.
No overly regular roof grids, perfect garden grids, clean vector map look, photorealism, 3D render look, toy miniature look, mobile-game tile look, fantasy styling, or modern architecture.

Output:
One clean 16:9 illustrated low 3/4 isomorphic background plate, no UI. Success means: north-up playable isomorphic perspective; closer to the original parish notebook style; broad roads and yards remain continuous and walkable; map-supported building footprints remain in approximate place; all visible walkable buildings have readable doors and thresholds; open fields remain open; suppressed admin boundaries and erased seams leave no physical trace; no chimneys/smoke/roof nubs; no people/animals/water/church/shop/text leakage.
