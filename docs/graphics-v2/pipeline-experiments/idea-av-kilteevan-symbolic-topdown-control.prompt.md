Use case: historical-scene
Asset type: minimal-boundary top-down illustrated control plate, native 16:9 desktop background plate, no UI

Primary request:
Create a cleaned, top-down, north-up illustrated control plate from the supplied historic map crop and cleaned no-admin crop. This is not the final isomorphic game image. It is a layout-preserving control layer for a later low 3/4 isomorphic render.

The goal is not to make a pretty finished estate plan. The goal is to preserve source-map topology while avoiding the previous failure where generated top-down controls turned uncertain field lines, erased administrative seams, and decorative enclosure edges into convincing physical walls.

Execution context:
Use only the images attached with this request and this text prompt. Do not use previous generated plates, prior experiment prompts, prior conversation context, file names, hidden assumptions, route graphs, or hand-authored location notes. Infer the location only from the map/control images and the generic rules below.

Input images and authority order:
Image 1: tight original historic Ordnance Survey-style map crop. Primary source evidence for broad roads/lanes, dark roof/building marks, planted areas, tree/scrub symbols, garden texture, field/parcel divisions, and orientation. Top of this image is north.
Image 2: matching tight cleaned no-admin crop. Highest veto authority for suppressed dotted/pecked/dashed administrative/survey linework. Soft gray erased seams, pale diagonal smears, and faint scars in this image are deletion artifacts, not terrain.
Image 3: cleaned slate-roof single-house style reference. Use only for restrained top-down roof texture and muted ink/watercolor material cues; do not copy the building as content.
Image 4: cleaned thatched/no-chimney single-house style reference. Use only for possible rough roof texture and no-chimney discipline; do not copy the building as content.
Image 5: tree/field watercolor style reference. Use only for soft open fields, uneven grass, scrub, and watercolor vegetation texture.

Camera and geometry:
Strict top-down orthographic plan view. No isometric perspective, no oblique camera, no visible vertical facades, no side walls, no cast 3D building shadows, no horizon, no sky, no vanishing point. Keep source-map top as final-image top: north remains up, east right, south bottom, west left. Do not rotate the plan into a prettier diagonal composition. Preserve relative positions, angles, footprint proportions, road widths, road junctions, planted areas, tree clusters, yard spaces, and the broad road/building/garden relationships.

Native 16:9 framing:
The supplied crop is already the intended local playable area. Produce a native 16:9 plate matching that local extent. Do not zoom out to a regional overview. Do not add synthetic side padding, mirrored margins, blurred edge extension, decorative borders, or arbitrary extra context. Preserve awkward edge entries/exits if roads, fields, or vegetation continue off-frame.

Generic map interpretation policy:
Ignore printed labels, place-name text, large letters, numerals, survey text, stains, paper dots, and typography as in-world objects.
Broad pale corridors are roads or lanes.
Dark filled or hatched rectangles attached to yards/lanes are buildings or outbuildings.
Dense round/conifer/scrub symbols are vegetation masses, orchard/scrub, or field-edge trees, not buildings.
Regular internal garden strokes are planting/bed texture, not walls and not paths unless the map clearly shows a broad walkable corridor.
Thin solid lines may be plot boundaries, hedge lines, low banks, ditches, overgrown walls, or survey parcel edges; they are not automatically roads, paths, fences, or stone walls.
Dotted, pecked, dashed, or dot-chain lines are likely administrative/survey boundaries unless corroborated by physical symbols; render no physical trace for them.

Administrative/survey boundary veto:
If a line or mark is suppressed, erased, pale, gray, diagonal, smeared, scar-like, or missing in Image 2, it must not become terrain in this control plate. Render ordinary open grass or paper-textured field wash there.
Unsupported administrative/survey boundaries must leave no physical trace: no hedge, no bush line, no stone wall, no fence, no ditch, no road, no path, no crop row, no ridge, no tree row, no shadow, no color seam, no decorative texture.
Only draw a boundary as physical if Image 1 or Image 2 independently supports it with paired road edges, tree/hedge symbols riding the line, wall/ditch hatching, enclosure-edge continuity, gate/yard relationships, or a clear domestic/garden compound edge. If uncertain, omit the physical trace.

Minimal-boundary control policy, highest priority:
This top-down control must be mostly terrain zones, roads, building footprints, planted/garden texture, and tree/scrub masses. It should not become a finished wall map.

Open fields:
Keep open fields open. Do not outline fields. Do not draw long straight walls through open grass. Do not trace every thin parcel line. Most ordinary field/parcel lines should disappear entirely or become only a barely visible grass-tone shift.
Use open watercolor field washes, mottled grass, sparse scrub, and soft vegetation texture. A later final-render model should not be tempted to turn the field texture into walls.

Roads:
Broad road/lane corridors should be clear muddy ochre-brown shapes with soft grass shoulders and subtle rut texture. Do not run continuous walls, hedges, stones, or fences along both sides of roads. Road edges may have occasional soft vegetation clumps where the map shows trees/scrub, but not a continuous border.

Domestic yards and building compounds:
If the map supports a domestic yard or building compound, show it as open worn ground, mud, grass, and building footprints. Use only a few short, broken, low boundary fragments if essential for legibility. Do not make neat enclosure walls.

Garden/orchard/nursery:
Mapped planted areas should read as planted beds, orchard/scrub texture, and soil/plant rows. Internal bed lines are plants and soil texture, not walls. The outer edge may be indicated with a very faint, broken, symbolic boundary or vegetation transition, but not a stone wall, not a palisade, not a fortress-like outline, and not continuous all the way around.

Buildings:
Buildings remain top-down roof or footprint shapes, not 3D volumes. Represent buildings only where supported by dark roof/building marks in Images 1-2. Keep mapped building footprints separated when the map separates them. Do not merge separate roof marks into a picturesque compound, and do not split a connected roof mark into decorative cottages.
Roofs should be simple plan-view dark slate/thatch/roof texture blocks. No facades, no doors, no windows in this top-down control.

Walkability/topology:
Roads, lanes, yards, gates, entrances, and open working spaces must remain continuous and unobstructed. Do not invent a web of new paths. Do not convert thin parcel lines into extra footpaths. Do not route paths around erased administrative seams. Where a route crosses a physical boundary, show a gap or opening rather than a collision. Do not place trees, buildings, walls, garden beds, or props in road centers, gate openings, or yard centers.

Roof/artifact discipline:
No visible chimneys anywhere, even in top-down roof shapes. No chimney-like stacks, roof nubs, vents, pipes, capstones, wall chimneys, isolated vertical posts, roof pegs, ridge boxes, smoke-hole marks, black puffs, or protrusions embedded in roofs or walls. No visible smoke. Roof texture may be mottled, patched, mossy, and hand-painted, but it must not form isolated vertical marks that could become chimneys in the next pass.

Style/medium:
Hand-inked watercolor over parchment, but restrained and control-friendly. Sepia/dark brown ink line, muted moss and olive greens, soft uneven grass washes, subtle muddy roads, rough vegetation blobs, visible paper grain, imperfect hand-painted edges. Use fewer hard outlines than AT1/AT2. Avoid tidy stone chains, perfect garden grids, clean vector-map boundaries, toy miniature clarity, or a finished cadastral estate-plan look.

Hard negative constraints:
No UI, labels, signs, map pins, visible text, copied survey numbers, copied style-reference objects, people, animals, carts, barrels, smoke, fog, weather effects, invented water unless the map clearly shows water, bridges unless the map clearly shows a water crossing, churches or graveyards unless the map clearly shows church/churchyard evidence, shopfronts or market squares, decorative chimneys, freestanding chimneys, chimneys embedded in walls, visible facades, isometric view, cast 3D shadows, semantic content copied from the style references, continuous stone-wall networks, continuous road borders, continuous garden fortress walls, or any physical trace along suppressed admin/survey scars.

Output:
One clean 16:9 top-down minimal-boundary illustrated control plate. Success means: north-up map topology preserved; broad roads remain broad and continuous; open fields remain open; suppressed admin boundaries and erased seams leave no physical trace; map-supported building footprints appear; planted areas and tree clusters remain where mapped; top-down boundaries stay sparse/symbolic rather than physical walls; no chimneys/smoke/roof nubs; no UI/text/people/animals/water/church/shop leakage.
