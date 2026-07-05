Use case: style-transfer
Asset type: scale-locked low-oblique orthographic/isomorphic game background plate, native 16:9 desktop, no UI

Primary request:
Create Cycle BQ E1. Edit BP E2 directly. The camera angle and overall art style are close, but BP E2 fails the true isomorphic/orthographic scale test: distant/top-of-frame trees are smaller than near/bottom trees. Correct that scale drift while preserving the low camera, crop, layout, roads, buildings, garden, doors, and notebook watercolor style.

Input images and roles:
Image 1: BP E2 render, edit target and content/style authority. Preserve its crop, scene layout, camera angle, roads, buildings, garden/orchard block, tree masses, doors, and overall notebook style.
Image 2: BP E2 scale-audit overlay. Diagnostic only. It shows the failure: constant-size marker rings/sprite rulers reveal that top/far trees are too small. Do not copy the colored markers or grid into the output.
Image 3: pure isomorphic scale-lock reference. Projection/scale law only. It shows that equal world objects and sprite rulers stay the same pixel size at every row of the image. Do not copy the colored markers, grid, or parchment blankness into the output.
Image 4: original illustrated parish notebook. Style target only. Use its loose watercolor/ink/paper material, not its UI, people, labels, bridges, river, shop, church, signs, animals, or scene content.

Local paths:

- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/pipeline-experiments/idea-bp-e2-kilteevan-art-last-grid-style-tighten.png
- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/pipeline-experiments/idea-bq-bp-e2-scale-audit-overlay.png
- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/pipeline-experiments/idea-bq-isomorphic-scale-lock-reference.png
- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/illustrated-parish-notebook.png

Hard correction:
Make the plate truly orthographic/isomorphic, not perspective. The camera may stay low and oblique, but there must be no distance-based scaling.

- Trees of the same kind should have the same crown diameter and trunk/branch stroke scale whether they are at the bottom, middle, or top of the frame.
- Shrubs, stones, road texture, garden plants, wall stones, and roof slates should not shrink merely because they are farther north/top-of-frame.
- A player sprite placed at any road, yard, or garden gate would use one constant pixel scale everywhere.
- Do not use atmospheric perspective, horizon perspective, or miniaturized far-background treatment.
- Do not fade the top/north content into a scenic backdrop. It remains playable map ground in the same orthographic scale as the foreground.

Keep from BP E2:

- exact 16:9 crop and low camera angle,
- main lower building and foreground outbuildings,
- upper building group,
- muddy road junction, left road, upper road, and right-side road,
- garden/orchard block and planting rows,
- existing tree mass positions,
- fitted plank doors on every visible walkable facade,
- no chimneys or roof protrusions,
- no visible grid, no colored marker rings, no sprite rulers.

Allowed changes:

- Enlarge/repaint only the visually miniaturized top/far tree crowns, branches, stones, road texture, and field texture so they match the near/mid object scale.
- Slightly simplify or crop top tree clusters if needed to avoid crowding after scale correction.
- Preserve the existing notebook wash and looseness; do not make the plate cleaner or more diagrammatic.

Forbidden changes:

- Do not move, add, remove, merge, or recompose roads, buildings, garden blocks, exits, or tree masses.
- Do not add extra roads, fences, walls, paths, buildings, water, people, animals, carts, barrels, signs, labels, UI, smoke, fog, sky, horizon, chimneys, roof nubs, churches, chapels, graveyards, shops, bridges, rivers, streams, ponds, or decorative scenic balancing.
- Do not make the top edge smaller, blurrier, paler, or more distant-looking than the bottom edge.
- Do not copy any colored overlay marker, grid line, label, or guide mark into the final output.

Output:
One clean 16:9 illustrated low-oblique orthographic/isomorphic background plate. Success means BP E2's good camera/art style survives, but top/far trees and map objects are no longer miniaturized; a constant-scale sprite could move anywhere on the walkable plate without rescaling.
