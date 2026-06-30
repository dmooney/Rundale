Use case: style-transfer
Asset type: close playable-area illustrated concept-art background plate, native 16:9 desktop, no UI

Primary request:
Create Cycle BS E1 for Beechwood. Start from the BR E1 Beechwood concept-art plate, but zoom the virtual camera out slightly so the visible doors match the door height in the original illustrated parish notebook concept art. This is a door-height calibration pass, not a new map interpretation.

Input images and roles:
Image 1: BR E1 Beechwood render. Edit target and primary style/layout base. Preserve its connected compound, road, yard, garden, trees, material palette, and notebook watercolor style.
Image 2: original illustrated parish notebook. Door-height and art-style target only. Match the scale of its readable shop/church/cottage doors relative to the full scene: clear human-scale doors, not close-up oversized doors and not tiny survey-map slits.
Image 3: close Beechwood topology/control crop. Layout evidence only, especially the connected compound and garden/yard relationship. Do not copy dark holes from this control as empty doorways.
Image 4: close Beechwood map source crop. Source/map evidence only for the compound footprint, road, yard, and garden placement.
Image 5: fixed thatched single-house door crop. Door/facade material reference only: visible fitted plank door, threshold, rough limewash, no dark void.
Image 6: fixed slate single-house door crop. Secondary door/facade material reference only.

Local paths:
- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/pipeline-experiments/idea-br-e1-beechwood-close-raised-camera-door-fixed-concept.png
- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/illustrated-parish-notebook.png
- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/pipeline-experiments/idea-br-beechwood-close-control.png
- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/pipeline-experiments/idea-br-beechwood-close-map-source.png
- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/style-crops/illustrated-style-low-camera-thatched-single-house-door-fixed.png
- /Users/dmooney/.codex/worktrees/a718/Rundale/docs/graphics-v2/style-crops/illustrated-style-low-camera-slate-single-house-door-fixed.png

Scale/camera requirement:
Zoom out only enough that the main visible plank doors are concept-art sized. Compared with BR E1, doors should be materially smaller, roughly about two-thirds as tall, while still clearly readable as wooden doors with thresholds. Use the original notebook doors as the standard. Keep the close playable feel; do not return to a high survey-board or estate-map camera.

Critical door rule:
Every visible person-sized opening on every walkable facade must contain a fitted wooden plank door and a threshold/step. Do not render black doorway voids. Do not render empty dark holes. Do not place a door beside an opening while leaving the opening empty. If an opening is dark, visible plank boards or cross-bracing must be inside that same opening.

Composition:
Keep the same Beechwood connected thatched house/yard compound as BR E1, but reveal a little more surrounding context around it: a touch more muddy road at left/lower edge, a little more garden/orchard edge to the right, and a little more tree/hedge mass. Preserve the same general orientation and 16:9 crop.

Style:
Match the original parish notebook concept art: loose sepia ink, mottled watercolor, parchment warmth, rough thatch, uneven limewashed walls, muddy road scumble, hand-painted garden texture, visible paper tooth, and dense but imperfect rural detail.

Keep:
- connected Beechwood compound topology,
- thatched roofs, limewashed walls, walled yard, muddy road, garden/orchard edge,
- no UI, no labels, no map lettering, no survey numbers,
- no people or animals,
- no smoke, fog, sky, horizon, or weather layer,
- no colored audit symbols or sprite markers.

Avoid:
No missing doors, no black doorway voids, no oversized close-up doors, no tiny illegible door slits, no chimneys, no roof nubs, no pipes, no vents, no signs, no shop/church/chapel/graveyard/bridge/river copied from the style reference, no extra buildings, no extra roads, no visible grid, no generic scenic balancing.

Output:
One clean 16:9 Beechwood concept-art background plate. Success means the overall style remains as warm and rich as BR E1, the camera is zoomed out a bit, the main doors match the original notebook door-height standard, and every visible building has fitted wooden plank doors rather than dark voids.
