Use case: historical-scene
Asset type: low 3/4 orthographic isomorphic game environment background plate, native 16:9 desktop, no UI

Primary request:
Create Cycle BM E4: repeat the E3 setup using the native z17 source crop, 55% crop, and y_squash-0.40 lower-camera cue, but treat road/path/wall classification as the main experiment. Keep the lower camera and close playable zoom, while reducing the specific E2/E3 failure where garden/internal lines and ambiguous boundaries became too many walls/fences/paths.

Input images and authority order:
Image 1: 55% closer top-down cleaned control plate. Broad organization aid only; fallible.
Image 2: native z17 historic map crop. Highest authority for road/wall/path evidence, dark building marks, tree/scrub symbols, garden texture, and topology.
Image 3: lower-resolution cleaned no-admin crop. Veto authority for deleted dotted/pecked/dashed admin/survey linework and erasure scars.
Image 4: y_squash-0.40 oblique pitch cue. Camera cue only; do not copy blank margins or strip composition.
Image 5: original illustrated parish notebook sample. Style and low-camera feel only.
Image 6: fixed slate-roof single-house reference. Style/material only.
Image 7: fixed thatched single-house reference. Style/material only.
Image 8: tree/field watercolor reference. Style/material only.

Local paths:

- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/pipeline-experiments/idea-bm-kilteevan-topdown-55-control-1672.png
- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/pipeline-experiments/idea-bm-kilteevan-z17-native-55-map-crop.png
- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/pipeline-experiments/idea-bm-kilteevan-playable-55-no-admin-crop.png
- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/pipeline-experiments/idea-bm-kilteevan-topdown-55-oblique-y040.png
- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/illustrated-parish-notebook.png
- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/style-crops/illustrated-style-low-camera-slate-single-house-door-fixed.png
- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/style-crops/illustrated-style-low-camera-thatched-single-house-door-fixed.png
- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/style-crops/illustrated-style-trees-fields.png

Camera and zoom target:
Keep the E2/E3 lower-camera win: strict very-low 3/4 orthographic/isomorphic game camera around 20-25 degrees above the ground plane, close playable crop, substantial facades roughly half or more of visible roof depth where orientation permits, north-up, parallel map edges, stable walkable ground plane, no horizon/sky/vanishing point/drone/survey-board/miniature-estate view.

Most important semantic rule:
Make wall authority rare. The output should not look like a walled estate, fortress garden, fenced garden maze, or stone-walled board game. If a line can plausibly be a planting row, soil row, plot mark, hedge remnant, ditch, grass tone break, erasure scar, administrative mark, or low-confidence field boundary, do not turn it into a wall, fence, or footpath. Leave it as mottled grass, muddy wear, broken hedge clumps, low vegetation, or soft planting texture.

Road/path interpretation:
Broad pale corridors and paired parallel corridor evidence are muddy rural roads or lanes. Paired pale/dashed corridors can be unwalled tracks: preserve their continuity as open mud, grass-worn ground, ruts, or a soft verge, not as stone walls. Do not border every road with walls. Do not make roads into clean beige ribbons; make them muddy, rutted, scumbled, and open.

Single-line interpretation:
Single solid lines are usually boundary/hedge/ditch/wall/plot edge/vegetation edge, not walkable paths. Render single lines conservatively as very low broken hedge fragments, ditches, grass color shifts, or nothing. Do not convert single lines into footpaths. Do not convert dotted, pecked, dashed, dot-chain, no-data, erasure, deletion scar, label remnant, or survey texture into any physical feature.

Garden/orchard handling:
The garden/orchard region should remain in the same place and have internal planted texture, but it must not become a hard-walled rectangle grid. Internal rows are planting, soil, vegetables, low shrubs, orchard texture, or uneven beds. They are not stone walls, fences, or footpaths. The outer garden edge may have only sparse, broken, overgrown low boundary hints where strongly supported. It must have gaps and soft edges; no continuous enclosure, no perfect perimeter, no chessboard.

Buildings and doors:
Render only source-supported buildings. Preserve approximate position, separation, and relationship to roads/yards. Every visible enterable facade must contain a fitted timber plank door with threshold; no dark voids. Buildings must not block roads, yard centers, gates, or thresholds.

Roof rule:
No chimneys, roof nubs, vents, pipes, capstones, wall stacks, roof pegs, ridge boxes, smoke holes, protrusions, black puffs, or visible smoke. Roofs are continuous rough slate or thatch planes only.

Notebook style:
Hand-inked watercolor over parchment: sepia ink, rough roof hatching, dirty limewash, muddy road scumbling, mottled olive fields, irregular grass strokes, paper grain, dark lower tree masses, broken hand-painted edges. Keep local texture dense and human-scale, but not fantasy art, not 3D render, not toy miniature, not clean mobile tile.

Hard negatives:
No UI, labels, text, signs, map pins, people, animals, carts, barrels, smoke, fog, weather, invented water, rivers, streams, ponds, bridges, churches, chapels, graveyards, shops, wells, chimneys, roof nubs, copied style-reference objects, scenic balancing buildings, extra roads, or extra footpaths. No fortress garden, no continuous road borders, no field-wall web, no perfect wall/fence grid.

Output:
One clean 16:9 illustrated low 3/4 isomorphic background plate. Success means: E2/E3-level lower camera and close crop; larger facades/doors; roads/yards walkable; map-supported buildings in approximate place; unwalled tracks remain open; single lines do not become paths; garden/internal rows become planting texture rather than walls; admin/no-data traces omitted; no chimneys/smoke/semantic leaks.
