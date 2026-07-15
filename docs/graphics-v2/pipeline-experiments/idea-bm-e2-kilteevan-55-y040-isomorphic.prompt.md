Use case: historical-scene
Asset type: low 3/4 orthographic isomorphic game environment background plate, native 16:9 desktop, no UI

Primary request:
Create one fresh finished illustrated background plate for a historical isomorphic game from the supplied historic map crop, cleaned crop, closer top-down control, and lower-pitch oblique cue. This is Cycle BM, an isomorphic-transform calibration experiment. The goal is lower camera and closer playable zoom than the BA/BJ/BH comparison plates while preserving the map-derived topology: north-up layout, broad road corridors, building group relationships, yards, planted garden/orchard region, tree masses, open fields, and omission of unsupported administrative/survey linework.

Execution context:
Use only the images attached with this request and this prompt. Do not use prior conversation context, hidden location notes, route graphs, or hand-authored place-specific interpretations. The pipeline must remain generic and data-driven.

Input images and authority order:
Image 1: closer top-down cleaned control plate for this exact crop. Use as broad terrain/material organization: approximate roads, buildings, planted/garden area, trees/scrub, yards, and open fields. Treat it as fallible, not source truth.
Image 2: historic Ordnance Survey-style map crop for this exact closer area. Highest authority for feature existence, orientation, broad road corridors, dark roof/building marks, planted areas, tree/scrub symbols, garden texture, and source topology. Top is north.
Image 3: matching cleaned no-admin crop. Highest veto authority for suppressed dotted/pecked/dashed administrative/survey linework. Soft gray erased seams, pale diagonal smears, and faint scars are deletion artifacts, not terrain.
Image 4: deterministic oblique pitch cue from Image 1. Camera/pitch cue only. Use it to understand how much the north-up ground plane should compress under the lower 3/4 camera. Do not copy beige margins, strip composition, blank bands, scan texture, text fragments, line artifacts, or exact crop.
Image 5: original illustrated parish notebook sample. Style and low playable camera feel only: sepia ink, watercolor wash, paper grain, muddy roads, readable facades, dense hand detail, rough vegetation, and notebook atmosphere. Do not copy UI, labels, people, church, graveyard, river, bridge, shop, signs, carts, animals, named places, chimneys, smoke, composition, or landmarks.
Image 6: fixed slate-roof single-house reference. Style/material only: low-camera limewashed facade, fitted timber plank door, threshold, hand ink, slate roof texture, no-chimney discipline.
Image 7: fixed thatched single-house reference. Style/material only: rough thatch, low camera, fitted timber plank door, threshold, no-chimney discipline.
Image 8: tree/field watercolor reference. Style/material only: soft open fields, uneven grass, hedges, scrub, field texture, watercolor vegetation.

Local paths for the input images:

- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/pipeline-experiments/idea-bm-kilteevan-topdown-55-control-1672.png
- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/pipeline-experiments/idea-bm-kilteevan-playable-55-map-crop.png
- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/pipeline-experiments/idea-bm-kilteevan-playable-55-no-admin-crop.png
- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/pipeline-experiments/idea-bm-kilteevan-topdown-55-oblique-y040.png
- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/illustrated-parish-notebook.png
- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/style-crops/illustrated-style-low-camera-slate-single-house-door-fixed.png
- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/style-crops/illustrated-style-low-camera-thatched-single-house-door-fixed.png
- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/style-crops/illustrated-style-trees-fields.png

Camera and zoom target:
Use a strict very-low 3/4 orthographic/isomorphic game camera around 20-25 degrees above the ground plane, lower and closer than BA/E1. Main building facades should be visually substantial, about half or more of visible roof depth where orientation permits. Keep rooftops visible and maintain a stable ground plane, but avoid any top-down, drone, survey-board, miniature estate-map, or strip-diagram feel. Keep north up and all map edges parallel; no horizon, no sky, no vanishing point.

Composition lock:
The supplied closer crop is the intended local playable area. Do not zoom back out to the older wider crop. Do not add off-crop context. Roads, walls, and boundaries may exit the frame naturally. Preserve awkward local crop behavior if roads, vegetation, gardens, or buildings enter/exit frame edges. Do not rearrange roads into a centered scenic Y, X, or postcard crossroads.

Map/control conflict rules:
Image 2 and Image 3 outrank Image 1. If the top-down control contains a continuous wall, hedge, field line, seam, road, path, crop row, or vegetation chain that is absent from the source/cleaned crops or aligns with a cleaned/suppressed administrative/survey scar, omit it or reduce it to ordinary open field wash. Do not preserve top-down-control mistakes just because they are visually coherent.

Roads, paths, walls, and linework:
Broad pale corridors in Images 2-3 are muddy rural roads or lanes. Keep them broad, continuous, mostly unfenced, and clear for character movement. Paired pale or dashed corridor evidence may indicate an unwalled route or track; preserve plausible unwalled route continuity as mud/grass wear rather than replacing it with walls. Single thin solid linework is usually a boundary, hedge, ditch, wall, plot edge, or vegetation edge, not a walkable route. Do not convert thin parcel lines, garden/internal lines, class-control boundaries, no-data/erasure swaths, or dotted administrative marks into paths. Unsupported dotted/pecked/dashed/dot-chain admin or survey boundaries must leave no physical trace: no wall, hedge, fence, ditch, path, crop row, shadow, color seam, or decorative texture.

Open-field-first boundary hierarchy:
Open fields must read open at first glance. Do not outline every field. Do not trace ordinary parcel lines as walls. Only clear domestic yards, building compounds, and planted garden/orchard/nursery enclosures may receive visible boundary treatment, and even those must be mixed, low, broken, irregular, and overgrown. Garden/internal rows are soil and planting texture, not stone walls and not extra paths unless clearly broad and walkable.

Buildings and doors:
Render only buildings supported by dark roof/building marks in Images 2-3 and compatible with Image 1's approximate footprint organization. Preserve approximate footprint size, separation, orientation, and road/yard relationships. Do not invent extra cottages, sheds, shops, barns, compounds, or decorative buildings to fill corners. Every visible walkable house, cottage, barn, byre, shed, or outbuilding that shows any facade must have one readable human-usable timber plank door on a visible facade, plus a small threshold connected to a yard or road. A door means an actual brown or weathered gray-brown timber slab or half-open plank door with vertical plank marks, not a black hole, vague shadow, wall stain, roof edge, or dark window. This includes small sheds and partial edge buildings if they read as enterable.

Absolute roof rule:
No visible chimneys anywhere. No random chimneys. No chimney-like stacks, roof nubs, vents, pipes, capstones, wall chimneys, isolated vertical posts, roof pegs, ridge boxes, smoke holes, black puffs, or protrusions embedded in roofs or walls. No visible smoke. Slate roofs must be continuous rough slate planes with inked texture only. Thatched roofs must be continuous rough thatch with no roof holes, no smoke holes, no protruding stacks, and no vertical objects.

Notebook art target:
Use uneven sepia/brown-black ink contours, scratchy field hatching, broken roof hatching, dirty stained limewash, dry-brush stone texture where stones actually remain, muddy ochre road scumbling, tiny irregular grass strokes, mottled olive watercolor fields, cool gray-blue shadows, softened paper grain, watercolor blooms, and imperfect hand-painted edges. The scene should have more local texture density and readable facades than a survey plate, but it must not become fantasy art, a clean 3D render, toy miniature, or mobile-game tile.

Hard negatives:
No UI, labels, visible text, signs, map pins, copied survey numbers, people, animals, carts, barrels, smoke, fog, weather effects, invented watercourses, rivers, streams, ponds, bridges, churches, chapels, graveyards, shops, market squares, wells, decorative chimneys, freestanding chimneys, chimneys embedded in walls, roof nubs, copied style-reference objects, or semantic content from the notebook sample. No picturesque/generic crossroads composition. No invented scenic balancing buildings. No extra roads or footpaths.

Output:
One clean 16:9 illustrated low 3/4 isomorphic background plate, no UI. Success means lower and closer than the BA comparison plate; facades and doors are materially larger; broad roads and yards remain continuous and walkable; map-supported building footprints remain in approximate place; open fields remain open; all visible walkable buildings have fitted plank doors and thresholds; suppressed admin/no-data areas leave no physical trace; no chimneys/smoke/roof nubs; no people/animals/water/church/shop/text leakage.
