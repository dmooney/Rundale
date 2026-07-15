Use case: historical-scene
Asset type: low 3/4 orthographic isomorphic game environment background plate, native 16:9 desktop, no UI

Primary request:
Create one fresh finished illustrated background plate for Cycle BM E3. Use the same 55%/y_squash-0.40 lower-camera transform tested in E2, but use the native z17 historic map crop as the highest-resolution source evidence. The goal is to keep E2's lower camera and closer zoom while improving cartographic accuracy: road corridors, wall/path distinctions, building positions, yard relationships, planted garden/orchard region, open fields, and omitted admin/survey linework.

Input images and authority order:
Image 1: 55% closer top-down cleaned control plate. Broad organization aid only; fallible, not source truth.
Image 2: native z17 historic map crop for the same Kilteevan area. Highest authority for visible source details, road/wall/path evidence, dark roof/building marks, planted area texture, tree/scrub symbols, and topology. Top is north.
Image 3: lower-resolution matching cleaned no-admin crop. Highest veto authority for suppressed dotted/pecked/dashed administrative/survey linework and deletion scars; do not treat its pale erased scars as terrain.
Image 4: deterministic y_squash-0.40 oblique pitch cue from Image 1. Camera/pitch cue only; do not copy beige margins or strip composition.
Image 5: original illustrated parish notebook sample. Style and low-camera feel only; do not copy semantic content.
Image 6: fixed slate-roof single-house reference. Style/material only: low-camera facade, fitted door, threshold, no chimney.
Image 7: fixed thatched single-house reference. Style/material only: thatch, fitted door, threshold, no chimney.
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

Camera and zoom:
Use a strict very-low 3/4 orthographic/isomorphic game camera around 20-25 degrees above the ground plane, matching E2's successful lower camera. Main building facades should be visually substantial, about half or more of visible roof depth where orientation permits. Keep rooftops visible, keep all walkable surfaces on one stable ground plane, keep north up, and keep map edges parallel. No horizon, sky, vanishing point, drone, top-down survey-board, miniature estate-map, or strip-diagram feel.

Composition:
The 55% closer crop is the local playable area. Do not zoom out. Roads, walls, and boundaries may exit the frame. Do not invent off-crop context or a balanced scenic crossroads. Keep building-yard-garden relationships from the source/control evidence.

Source-resolution rule:
Image 2 is the highest-resolution source. Use it to resolve ambiguous roads, walls, paths, building marks, tree symbols, and garden texture. Image 3 is lower-res but valuable as a veto for deleted admin/survey marks. If Image 2 and Image 3 disagree because Image 3 removed dotted/pecked/dashed admin linework, obey the veto and leave no physical trace. If Image 2 shows paired corridor/route evidence that survived the crop, preserve it as walkable worn ground rather than replacing it with a wall.

Roads, paths, and walls:
Broad pale corridors and paired line corridors are muddy rural roads or lanes. Paired pale or dashed corridor evidence may be an unwalled route or track; preserve plausible unwalled route continuity as mud/grass wear. Single thin solid linework is usually boundary/hedge/ditch/wall/plot edge/vegetation edge, not a walkable path. Do not convert thin parcel lines, garden/internal lines, class-control boundaries, no-data swaths, or dotted admin marks into paths. Unsupported dotted/pecked/dashed/dot-chain admin or survey boundaries leave no trace: no wall, hedge, fence, ditch, path, crop row, shadow, color seam, or texture.

Open-field and garden handling:
Open fields read open at first glance. Do not outline every field. Clear domestic yards and planted garden/orchard/nursery enclosures may receive broken low boundary treatment, but garden/internal rows are soil and planting texture, not stone walls and not extra paths unless clearly broad and walkable. Make garden texture handmade, irregular, and organic, not a fortress or chessboard.

Buildings and doors:
Render only buildings supported by dark roof/building marks. Preserve approximate footprint size, separation, orientation, and road/yard relationships. Every visible enterable building facade must have one readable fitted timber plank door and threshold. A door is a brown or weathered gray-brown timber slab or half-open plank door with vertical plank marks, not a black hole or shadow. Include sheds/edge buildings if they read as enterable.

Roof rule:
No chimneys, chimney-like stacks, roof nubs, vents, pipes, capstones, wall stacks, roof pegs, ridge boxes, smoke holes, black puffs, protrusions, or visible smoke. Slate roofs are continuous rough slate planes; thatch is continuous rough thatch.

Notebook art target:
Hand-inked watercolor over parchment: sepia ink, rough roof hatching, dirty limewash, dry-brush stone only where stones remain, muddy road scumbling, mottled olive fields, irregular grass strokes, visible paper grain, dark lower tree masses, and imperfect edges. More local texture and readable facades than BA, but not fantasy art, 3D render, toy miniature, or clean mobile tile.

Hard negatives:
No UI, labels, text, signs, map pins, people, animals, carts, barrels, smoke, fog, weather, invented water, rivers, streams, ponds, bridges, churches, chapels, graveyards, shops, wells, chimneys, roof nubs, copied style-reference objects, scenic balancing buildings, extra roads, or extra footpaths.

Output:
One clean 16:9 illustrated low 3/4 isomorphic background plate. Success means E2-level lower camera plus better source fidelity from native z17 evidence: larger facades/doors, continuous walkable roads/yards, map-supported buildings in approximate place, open fields open, garden not over-walled, admin/no-data traces omitted, no chimneys/smoke/semantic leaks.
