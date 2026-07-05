Use case: historical-scene
Asset type: top-down cleaned illustrated control plate, native 16:9 desktop background plate, no UI

Primary request:
Create a cleaned, top-down, north-up illustrated plan plate from the supplied historic map crop and cleaned no-admin crop. This is not the final isomorphic image. It is a layout-preserving art/control plate for a later isomorphic conversion step. The result should translate map symbols into readable painted terrain, road corridors, physical boundaries, planted areas, building footprints, gardens, yards, and vegetation while preserving the source topology as exactly as possible.

Execution context:
Use only the images attached with this request and this text prompt. Do not use previous generated plates, prior experiment prompts, prior conversation context, file names, hidden assumptions, or hand-authored location notes. Infer the location only from the map/control images and the generic rules below.

Input images and authority order:
Image 1: tight original historic Ordnance Survey-style map crop. Primary source evidence for broad roads/lanes, dark roof/building marks, enclosed yards, planted areas, tree/scrub symbols, field/parcel divisions, and orientation. Top of this image is north.
Image 2: matching tight cleaned no-admin crop. Physical-linework control. Dotted/pecked/dashed administrative or survey dot chains have been suppressed. Soft gray erased seams, pale diagonal smears, and faint scars in this image are deletion artifacts, not terrain.
Image 3: original illustrated parish notebook sample. Style only: hand ink, watercolor texture, paper grain, muddy rural material feel, visual density, and historical notebook atmosphere. Do not copy its UI, labels, people, church, graveyard, river, bridge, shop, signposts, carts, animals, named places, chimneys, smoke, composition, or landmarks.
Image 4: cleaned slate-roof single-house style reference. Use only for ink/watercolor material treatment and roof/facade texture cues; do not copy the building as content.
Image 5: cleaned thatched/no-chimney single-house style reference. Use only for thatch texture, rough eaves, and no-chimney discipline; do not copy the building as content.
Image 6: tree/field watercolor style reference. Use only for soft open fields, uneven grass, hedges, scrub, and watercolor vegetation texture.

Camera and geometry:
Strict top-down orthographic plan view. No isometric perspective, no oblique camera, no visible vertical facades, no side walls, no cast 3D building shadows, no horizon, no sky, no vanishing point. Keep source-map top as final-image top: north remains up, east right, south bottom, west left. Do not rotate the plan into a prettier diagonal composition. Preserve relative positions, angles, footprint proportions, road widths, road junctions, enclosed planting, tree clusters, yard spaces, and physical boundaries.

Native 16:9 framing:
The output should be a native 16:9 plate using the supplied tight crop as the intended local playable area. Do not zoom back out to a regional overview. Do not add synthetic side padding, mirrored margins, blurred edge extension, cloned fields, decorative borders, or arbitrary extra context. Keep enough real north/top content from the crop so the later isomorphic tilt has terrain to work with, but preserve the awkward local crop if features enter or exit at edges.

Generic map interpretation policy:
Ignore printed labels, place-name text, large letters, numerals, survey text, stains, paper dots, and typography as in-world objects.
Broad pale corridors are roads or lanes.
Dark filled or hatched rectangles attached to yards/lanes are buildings or outbuildings.
Dense round/conifer/scrub symbols are vegetation masses, orchard/scrub, or field-edge trees, not buildings.
Regular internal garden strokes are planting/bed texture, not walls and not paths unless the map clearly shows a broad walkable corridor.
Thin solid lines may be plot boundaries, hedge lines, low banks, ditches, overgrown walls, or survey parcel edges; they are not automatically roads or stone walls.
Dotted, pecked, dashed, or dot-chain lines are likely administrative/survey boundaries unless corroborated by physical symbols; render no physical trace for them.

Administrative/survey boundary rule:
Historic OS-style map keys include dotted, pecked, dashed, or dot-chain boundaries that can mark non-physical administrative or survey divisions such as townland, parish, barony, county, estate, or parcel boundaries. These are not terrain. If a dotted/pecked/dashed line lacks independent physical evidence, it must disappear into field texture. Do not render it as bushes, hedges, walls, fences, ditches, roads, paths, tree rows, crop rows, ridges, shadows, color seams, or decorative texture. Only draw a dotted/pecked/dashed line as physical when the original map also shows corroborating physical evidence: tree/hedge symbols riding the line, paired road edges, wall/ditch hatching, enclosure-edge continuity, gate/yard relationship, or another physical map mark. If uncertain, omit it as a physical feature.

Open-field boundary hierarchy:
Open fields should remain visually open. Ordinary thin field/parcel lines are uncertain cartographic boundaries, not automatic walls. Render most ordinary thin lines as no visible feature, faint grass color shifts, shallow drainage dips, broken hedge clumps, low overgrown banks, scattered scrub, or subtle field texture changes. They should be easy to overlook at first glance.
Only the clearest domestic yards, building compounds, and planted garden/orchard/nursery enclosures may receive visible boundary treatment. Even there, prefer mixed, low, broken, irregular boundaries: short stone wall fragments, gaps, hedges, earth banks, rough gate openings, or overgrown wall remnants.
Do not create a connected stone-wall network across open fields. Do not outline every field. Do not run stone walls along both sides of every road. Do not make a chessboard of walls. Do not trace long straight walls through open grass unless the map shows a strong enclosed yard/garden/compound.

Top-down translation:
Buildings remain top-down roof or footprint shapes, not 3D volumes. Represent primary buildings, sheds, barns, byres, stables, walled yards, or ambiguous ancillary structures only where supported by dark roof/building marks in Images 1-2. Keep mapped building footprints separated when the map separates them. Do not merge separate roof marks into one picturesque compound, and do not split a connected roof mark into decorative cottages.
Roads and lanes become matte ochre-brown dirt corridors with soft shoulders and subtle rut/stone texture.
Single thin physical parcel lines become modest hedges, banks, ditches, overgrown walls, or grass changes only when supported by the map; otherwise they stay faint or invisible.
Enclosed planted areas become top-down gardens, orchards, nurseries, beds, or planted yards according to the map symbols.
Tree symbols become top-down tree canopies, scrub clusters, or hedgerow clumps placed where the map shows them.

Walkability/topology:
Roads, lanes, yards, gates, entrances, and thresholds visible in the crop must remain continuous and unobstructed. Do not invent a web of new paths. Do not convert thin parcel lines into extra footpaths. Do not route paths around erased administrative seams. Where a route crosses a physical boundary, show a gap, gate, or opening rather than a collision. Do not place trees, buildings, walls, garden beds, or props in road centers, gate openings, or yard centers.

Roof and artifact discipline for this top-down control:
No visible chimneys anywhere, even in top-down roof shapes. No chimney-like stacks, roof nubs, vents, pipes, capstones, wall chimneys, isolated vertical posts, roof pegs, ridge boxes, smoke-hole marks, black puffs, or protrusions embedded in roofs or walls. No visible smoke. Roof/footprint texture may be mottled, patched, mossy, and hand-painted, but it must not form isolated vertical marks that could become chimneys in the next pass.

Style/medium:
Hand-inked watercolor over parchment. Sepia/dark brown ink line, muted moss and olive greens, cream/gray stone and limewash cues, soft uneven grass washes, restrained top-down roof hatching, muddy ochre roads, rough vegetation blobs, visible paper grain, imperfect hand-painted edges. It should be handmade and visually rich, but clean enough to serve as a topology control image.

Hard negative constraints:
No UI, no labels, no signs, no map pins, no visible text, no copied survey numbers, no copied style-reference objects, no people, no animals, no carts, no barrels, no smoke, no fog, no weather effects, no invented water unless the map clearly shows water, no bridges unless the map clearly shows a water crossing, no churches or graveyards unless the map clearly shows church/churchyard evidence, no shopfronts or market squares, no decorative chimneys, no freestanding random chimneys, no chimneys embedded in walls, no visible facades, no isometric view, no cast 3D shadows, no semantic content copied from the illustrated notebook sample.

Output:
One clean 16:9 top-down illustrated control plate. Success means: north-up map topology preserved; broad roads remain broad and continuous; suppressed admin boundaries leave no physical trace; open fields stay open; only map-supported buildings appear; planted areas and tree clusters remain where mapped; no chimneys/smoke/roof nubs; no UI/text/people/animals/water/church/shop leakage.
