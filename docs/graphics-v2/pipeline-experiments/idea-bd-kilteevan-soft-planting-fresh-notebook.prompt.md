Use case: historical-scene
Asset type: low 3/4 orthographic isomorphic game environment background plate, native 16:9 desktop, no UI

Primary request:
Create one fresh finished illustrated background plate for a historical isomorphic game from the supplied tight historic map crop, cleaned no-admin crop, top-down cleaned control, and deterministic soft-planting control. The goal is to match the original illustrated parish notebook background-plate look while preserving the map-derived topology discipline from the Cycle M/Q family: north-up layout, broad roads, building positions, yard relationships, garden/orchard region, tree masses, open fields, and no physical trace for unsupported administrative/survey boundaries.

This is a fresh final render. Do not edit or copy any previous isomorphic plate. Do not use prior rendered plates as layout references or style references. The supplied controls are aids, not source truth.

Execution context:
Use only the attached images and this prompt. Do not use prior conversation context, hidden location notes, route graphs, or hand-authored place-specific interpretations. The pipeline must remain generic and data-driven. Infer location content from the map/control images, not from named-place memory.

Input images and authority order:
Image 1: tight original historic Ordnance Survey-style map crop. Highest authority for feature existence, orientation, broad road corridors, dark roof/building marks, planted areas, tree/scrub symbols, garden texture, and source topology. Top is north.
Image 2: matching tight cleaned no-admin crop. Highest veto authority for suppressed dotted/pecked/dashed administrative/survey linework. Soft gray erased seams, pale diagonal smears, and faint scars are deletion artifacts, not terrain.
Image 3: generated top-down cleaned control plate for this same tight map crop. Use it only as broad organization for approximate roads, buildings, planted/garden area, trees/scrub, yards, and open fields. Treat it as fallible because generated controls can over-regularize walls, crop rows, seams, and boundaries.
Image 4: deterministic soft-planting control generated from Image 2 and original-vs-cleaned comparison. This is a material and suppression cue, not a scene and not a route graph. Interpret it as:

- pale/medium green base = ordinary open field or grass context,
- muted olive/dark green areas = soft planting, garden/orchard texture, scrub, hedgerow fragments, or vegetation masses; not hard walls,
- extremely faint brown/gray linework = weak source-map evidence only, not automatically physical boundaries,
- gray-green scars or diagonal swaths = suppressed/no-data/admin-deletion zones; render ordinary open grass or field wash there, never roads, walls, paths, hedges, ridges, seams, shadows, or features,
- any weak tan/beige traces = soft material transition only; not paths, not walls, not roads unless the raw/cleaned maps independently show a broad road.
  Image 5: deterministic oblique warp of the cleaned no-admin crop. Camera/pitch cue only. It shows how a north-up ground plane compresses under a low 3/4 camera. Do not copy beige margins, strip composition, scan texture, text fragments, line artifacts, or erased seams.
  Image 6: original illustrated parish notebook sample. Style and low playable camera feel only: sepia ink, watercolor wash, paper grain, muddy roads, readable facades, dense hand detail, rough vegetation, and notebook atmosphere. Do not copy UI, labels, people, church, graveyard, river, bridge, shop, signposts, carts, animals, named places, chimneys, smoke, composition, or landmarks.
  Image 7: cleaned slate-roof single-house style reference. Use only for limewashed rural facade, timber plank door, threshold, hand ink, slate roof texture, building scale, and no-chimney discipline.
  Image 8: cleaned thatched/no-chimney single-house style reference. Use only for possible thatch, rough eaves, timber plank door, threshold, and no-chimney discipline.
  Image 9: tree/field watercolor style reference. Use only for soft open fields, uneven grass, hedges, scrub, field texture, and watercolor vegetation.

Core success definition:
The output should feel like a UI-free version of the original illustrated parish notebook scene, but the layout must still read as the supplied historic map/control crop rather than a generic picturesque crossroads. If style and topology conflict, preserve topology first. If topology and boundary material conflict, raw/cleaned map evidence wins for existence while Image 4 wins for material interpretation: soft planting zones should not become walls, roads, or extra paths.

Absolute camera target:
Strict low 3/4 orthographic/isomorphic game camera around 30-35 degrees above the ground plane. This is not top-down and not a high survey plate. Show rooftops plus unmistakable vertical facades, plank doors, thresholds, wall side faces only where physical walls truly exist, gate posts where supported, dark lower tree masses, and playable road/yard surfaces. Keep all walkable surfaces on one stable ground plane. Parallel map edges remain parallel; no vanishing point.
Keep north up: source-map top and top-down-control top remain toward the top/north of the final image; east is right, south is bottom, west is left. The camera is south of the scene looking north. The ground plane may compress toward the top, but do not rotate the ground plan into a prettier diagonal composition. No horizon, no sky, no cinematic perspective, no fisheye, no drone/aerial-survey feel.

Composition lock:
The tight 16:9 map crop is the intended local playable area. Do not zoom out, widen the context, or invent a broader scenic local plan. Do not add off-crop context. Do not create a balanced scenic village or picturesque centered crossroads. Preserve awkward local crop behavior if roads, vegetation, gardens, or buildings enter/exit the frame edges.
Roads and lanes should continue naturally off-frame instead of being rearranged into a centered Y, X, or postcard crossroads. Images 1-2 outrank controls for crop extent, feature placement, and whether a line is real.

Soft-planting and boundary-material interpretation:
Image 4 is designed to prevent the BA/BC failure where garden/internal lines became hard walls. Use it aggressively for material meaning:

- Olive/green dense areas should render as planted texture, shrubs, orchard/garden growth, broken hedge clumps, scrub, or watercolor vegetation.
- Do not trace the edges of olive/green planting areas as stone walls.
- Do not turn garden/internal lines into walls, paths, or fences unless Images 1-2 clearly show a physical domestic/garden edge.
- Weak tan/beige traces in Image 4 are not paths and not road borders; treat them as soft material transition unless Images 1-2 show a broad pale road there.
- Gray-green scars are no-data/admin-deletion marks and must leave no physical trace.
- Faint brown/gray source linework in Image 4 is weak evidence only; most thin lines should disappear into watercolor field texture.

Map/control conflict rules:
Image 3 can guide approximate organization, but Images 1-2 veto feature existence and Image 4 controls material interpretation.
If Image 3 contains a continuous wall, hedge, field line, seam, road, path, crop row, or vegetation chain that Image 4 classifies as soft planting/no-data/weak linework and Images 1-2 do not strongly support as a physical feature, do not render it as a wall or path.
If Images 1-2 show only a pale erased seam, soft gray deletion scar, diagonal smear, dot-chain removal mark, label remnant, survey text, or paper texture, render open grass or ordinary field texture there.
Do not preserve top-down-control mistakes just because they are visually coherent.

Administrative/survey boundary rule:
Dotted, pecked, dashed, or dot-chain linework on historic maps can mark non-physical administrative/survey divisions: townland, parish, barony, county, estate, parcel, or survey boundaries. Unsupported admin/survey boundaries are not terrain and must leave no physical trace. Do not render them as hedges, bushes, walls, fences, ditches, roads, paths, crop rows, ridges, tree rows, shadows, seams, color changes, or decorative texture.
Only draw a boundary as physical when supported by independent map evidence: paired road edges, tree/hedge symbols riding the line, wall/ditch hatching, enclosure-edge continuity, gate/yard relationships, or a clearly domestic/garden compound edge. If uncertain, omit the physical trace.

Open-field-first boundary hierarchy:
Open fields must read open at first glance. Do not outline every field. Do not trace ordinary parcel lines as walls. Do not run continuous wall chains through grass. Do not run continuous walls along both sides of every road. Do not make a chessboard of walls.
Use mottled watercolor grass, sparse scrub, soft tone changes, shallow dips, and broken low vegetation for uncertain field texture. Most thin field lines should disappear.
Only clear domestic yards, building compounds, and planted garden/orchard/nursery enclosures may receive visible boundary treatment, and even those must be mixed, low, broken, and irregular: short wall fragments only where strongly supported, hedge clumps, earth banks, gaps, rough gate openings, overgrown remnants. No perfect enclosure, no fortress garden, no continuous palisade, no clean stone chain.

Roads, paths, and walkability:
Broad pale corridors in Images 1-2 are muddy rural roads or lanes. Keep them broad, continuous, mostly unfenced, and clear for character movement. Use soft grass shoulders, wheel ruts, stones, damp muddy scumbles only in roads, and uneven worn edges.
Do not invent decorative footpaths. Do not convert thin parcel lines, garden/internal lines, class-control boundaries, tan/beige soft bands, or no-data/erasure swaths into paths. Do not route paths around erased admin seams. Do not dead-end a road against a seam or line artifact. Where a route crosses a physical boundary, show a real gap, gate, opening, or worn threshold. Never place buildings, walls, trees, garden beds, props, or vegetation masses in road centers, gate openings, entrances, or yard centers.

Buildings:
Render only buildings supported by dark roof/building marks in Images 1-2 and compatible with Image 3's approximate footprint organization. Preserve approximate footprint size, separation, orientation, and road/yard relationships. Do not invent extra cottages, sheds, shops, barns, compounds, or decorative buildings to fill empty corners.
Use humble early-19th-century rural Irish vernacular: low limewashed stone or rough stone walls, patched plaster, slate or thatch where plausible, low eaves, small dark windows, uneven walls, muddy thresholds, service outbuildings as modest sheds/byres/barns/stables.
DOORS ON OPENINGS: Every visible walkable house, cottage, barn, byre, shed, or outbuilding that shows any facade must have one readable human-usable timber plank door on a visible facade, plus a small threshold connected to a yard or road. If you paint any person-sized dark vertical opening, doorway, entry gap, shed mouth, barn mouth, or black rectangular hole in a wall, put a visible brown or weathered gray-brown timber plank door directly on that opening, fitted inside the opening, with vertical plank marks or a half-open plank slab. Do not leave empty black door holes. Do not imply a doorway with a shadow only. Do not place a door beside the opening while the opening remains empty. Do not mistake a window shadow, wall stain, roof edge, fence gap, or cart shadow for a door. This includes small sheds, side buildings, foreground buildings, background buildings, and partial edge buildings if they read as enterable.
Buildings must not sit in roads. Buildings must not block lanes, yard centers, gates, or thresholds.

Absolute roof rule:
No visible chimneys anywhere. No random chimneys. No chimney-like stacks, roof nubs, vents, pipes, capstones, wall chimneys, isolated vertical posts, roof pegs, ridge boxes, smoke holes, black puffs, or protrusions embedded in roofs or walls. No visible smoke.
If uncertain whether to add a chimney, do not add one.
Slate roofs must be continuous rough slate planes with inked tile texture only.
Thatched roofs must be continuous rough thatch with no roof holes, no smoke holes, no protruding stacks, and no vertical objects.
Roof texture may be mottled, patched, mossy, and hand-painted, but it must not form isolated vertical marks.

Planted areas and vegetation:
Mapped garden/orchard/nursery/internal planting should become planted beds, low shrubs, orchard rows, vegetable rows, scrub, and garden texture with handmade irregularity. Internal bed lines are soil, planting rows, and vegetation texture, not stone walls and not extra paths unless clearly broad and walkable.
The garden/orchard should stay in the same region and keep a planted-bed feeling, but it should look hand-painted and organic rather than mechanically diagrammed. Avoid perfect grid geometry, chessboard rows, hard fenced rectangles, fortified rings, or terraced stone boxes.
Mapped tree/scrub symbols become tree canopies, dark lower masses, trunks where visible, scrub clumps, and sparse hedgerow fragments where supported. Do not turn every tree symbol chain into a continuous wall. Do not fill roads with trees.

Notebook art target:
The final plate should feel like the original illustrated parish notebook environment after the UI has been removed. Use uneven sepia/brown-black ink contours, scratchy field hatching, broken roof hatching, dirty stained limewash, dry-brush stone texture where stones actually remain, muddy ochre road scumbling, tiny irregular grass strokes, mottled olive watercolor fields, cool gray-blue shadows, softened paper grain, watercolor blooms, and imperfect hand-painted edges. Increase local value variation and hand-drawn texture. Roads should be rutted and hand-scrubbed rather than smooth pale ribbons. Fields should be blotchy and varied rather than uniform green. Walls and hedges should be rough, broken, low, irregular, sparse, and overgrown where present.
The scene should have more local texture density and readable facades than a survey plate, but it must not become a fantasy illustration, clean 3D render, toy miniature, or mobile-game tile.

Hard negative constraints:
No UI, labels, visible text, signs, map pins, copied survey numbers, people, animals, carts, barrels, smoke, fog, weather effects, invented watercourses, rivers, streams, ponds, bridges, churches, chapels, graveyards, shops, market squares, wells, decorative chimneys, freestanding chimneys, chimneys embedded in walls, roof nubs, copied style-reference objects, or semantic content from the notebook sample.
No empty person-sized building openings. No black door holes without actual plank doors on the holes.
No physical trace along suppressed dotted/pecked/admin/survey linework from Image 2 or gray-green no-data/suppressed zones from Image 4.
No picturesque/generic crossroads composition. No invented scenic balancing buildings. No extra roads or footpaths.
No overly regular roof grids, perfect garden grids, clean vector map look, photorealism, 3D render look, toy miniature look, mobile-game tile look, fantasy styling, modern architecture, continuous field outlines, continuous road borders, fortress-like garden boundaries, or hard wall outlines around soft planting zones.

Output:
One clean 16:9 illustrated low 3/4 isomorphic background plate, no UI. Success means: north-up playable isomorphic perspective; closer to the original parish notebook style; broad roads and yards remain continuous and walkable; map-supported building footprints remain in approximate place; every person-sized visible opening on every visible walkable building has a visible timber plank door fitted into that opening plus a threshold; open fields remain open; soft-planting control greens become planted texture rather than walls, roads, or extra paths; suppressed admin/no-data areas leave no physical trace; no chimneys/smoke/roof nubs; no people/animals/water/church/shop/text leakage.
