Use case: style-transfer
Asset type: art-last low-camera isomorphic game background plate, native 16:9 desktop, no UI

Primary request:
Create Cycle BP E1. Test the reversed pipeline order: use the perspective-first BO E2 plate as the geometry/content base, use the hard isomorphic grid as a projection check, then apply the original illustrated parish notebook art style as the final step. The goal is to recover the original notebook art feel that broke down in BO while keeping a hard low-oblique orthographic/isomorphic projection.

Input images and roles:
Image 1: BO E2 render, edit target and content/geometry base. Preserve its buildings, roads, garden/orchard region, tree masses, north/background content, low camera scale, doors, no-chimney behavior, and crop.
Image 2: BO E2 hard isomorphic grid-check overlay. Projection law only. The final image should satisfy this parallel-grid check but must not show the grid.
Image 3: pure hard isomorphic grid reference. Projection law only: two shallow low-oblique ground-axis line families, all perfectly parallel, no convergence, no fisheye.
Image 4: original illustrated parish notebook. Global art style target only: hand-inked watercolor, parchment, loose rural texture, warm imperfect paper, rough ink, muddy roads, dense vegetation, readable low-camera facades. Do not copy its UI, people, signs, church, shop, river, bridge, labels, or props.
Image 5: fixed slate single-house crop. Door/facade/limewash/slate/no-chimney material reference only.
Image 6: fixed thatched single-house crop. Thatch/facade/door/no-chimney material reference only.
Image 7: cleaned field/wall material crop. Soft field/hedge/wall material reference only, not layout.
Image 8: cleaned roof/wall or yard material crop. Surface texture reference only, not layout.

Local paths:

- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/pipeline-experiments/idea-bo-e2-kilteevan-bn-e2-soft-orthographic-rectify.png
- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/pipeline-experiments/idea-bp-bo-e2-hard-isomorphic-grid-check.png
- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/pipeline-experiments/idea-bp-hard-isomorphic-grid-reference.png
- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/illustrated-parish-notebook.png
- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/style-crops/illustrated-style-low-camera-slate-single-house-door-fixed.png
- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/style-crops/illustrated-style-low-camera-thatched-single-house-door-fixed.png
- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/style-crops/illustrated-style-field-wall-no-animals.png
- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/style-crops/illustrated-style-wall-roof-no-props.png

Hard isomorphic check:
The final image must read as a low oblique orthographic/isomorphic game plate:

- no fisheye, no barrel distortion, no bowed roads, no curved garden rows,
- no vanishing point, no perspective convergence, no horizon, no sky,
- roof ridges, wall edges, garden rows, road edges, and field boundaries align to shallow parallel projection families like Images 2-3,
- keep the camera low enough that facades and doors remain large,
- do not return to a high survey-board or miniature estate-map look.

The grid is an invisible check, not output content. Do not draw any grid lines, guide lines, colored axes, rulers, labels, or overlay marks in the final image.

Art-last style target:
Repaint the BO E2 scene into the original illustrated parish notebook look:

- warm parchment ground and sepia ink,
- loose hand-drawn linework, not clean digital contouring,
- watercolor wash with visible paper grain,
- mottled olive fields and hedges,
- muddy road scumble with pale puddled ruts,
- dense irregular vegetation made of layered brush marks,
- rough limewashed walls with stone texture,
- slate/thatch roof texture with broken hand hatching,
- less clean/3D/diagrammatic than BO E2,
- more of the original notebook's lively hand-painted imperfection.

Content and topology invariants:
BO E2 remains the source of truth for what exists and where it is:

- same main lower building,
- same foreground sheds/outbuildings,
- same upper building group,
- same muddy road junction and right-side road,
- same garden/orchard block and planting rows,
- same tree masses and open fields,
- same north/background content from the north-extended source.

Do not invent, remove, merge, move, or recompose buildings, roads, paths, walls, fields, garden blocks, tree masses, or the background. Do not add the notebook's church, bridge, river, shop, people, UI, labels, signs, carts, animals, smoke, or scenic props.

Wall/path/garden rule:
Do not make BO E2's garden and field edges harder. If anything, soften them toward hand-painted soil/planting texture while keeping the grid-aligned projection. Roads remain open muddy wear. Garden rows remain planting/soil texture, not stone walls or footpaths. Boundaries may be faint hedges/ditches/rough stone only where already present in BO E2; no new fences or continuous post-and-rail network.

Doors and roofs:
Every visible enterable facade must still have a fitted timber plank door plus threshold. No black doorway voids. No chimneys, roof nubs, vents, pipes, posts, smoke holes, black puffs, or visible smoke. Roofs remain continuous rough slate/thatch planes.

Hard negatives:
No visible grid, UI, labels, text, copied survey numbers, signs, map pins, people, animals, carts, barrels, smoke, fog, weather, sky, horizon, invented water, rivers, streams, ponds, bridges, churches, chapels, graveyards, shops, wells, chimneys, roof nubs, extra roads, extra buildings, extra fences, extra walls, or generic scenic balancing.

Output:
One clean 16:9 illustrated low oblique orthographic/isomorphic background plate. Success means the art style moves visibly back toward the original illustrated parish notebook while the geometry still passes the hard isomorphic grid check.
