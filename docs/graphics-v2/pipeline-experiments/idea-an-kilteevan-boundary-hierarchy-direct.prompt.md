# Cycle AN Kilteevan Boundary Hierarchy Direct Prompt

Generated with the built-in `image_gen` tool from a clean-context graphics worker. This is a direct control experiment: no prior generated plate was used as an edit target.

## Input Roles

1. `/Users/dmooney/Rundale/docs/graphics-v2/pipeline-experiments/idea-ah-kilteevan-z17-map-crop.png` - original historic map crop; primary layout evidence.
2. `/Users/dmooney/Rundale/docs/graphics-v2/pipeline-experiments/idea-aj2-kilteevan-dot-suppressed-no-admin-map-crop.png` - cleaned no-admin map crop; physical-linework control and deletion-artifact warning.
3. `/Users/dmooney/Rundale/docs/graphics-v2/pipeline-experiments/idea-aj2-kilteevan-dot-suppressed-no-admin-oblique-raw-warp.png` - oblique cleaned control; camera/pitch cue only.
4. `/Users/dmooney/Rundale/docs/graphics-v2/illustrated-parish-notebook.png` - original notebook art-direction reference only.
5. `/Users/dmooney/Rundale/docs/graphics-v2/style-crops/illustrated-style-low-camera-slate-single-house-door-clean.png` - slate-roof rural cottage material, facade, and doorway reference.
6. `/Users/dmooney/Rundale/docs/graphics-v2/style-crops/illustrated-style-low-camera-thatched-single-house-door-clean.png` - thatch, rough eaves, no-chimney, and dark doorway reference.
7. `/Users/dmooney/Rundale/docs/graphics-v2/style-crops/illustrated-style-trees-fields.png` - tree, field, hedge, scrub, and watercolor vegetation reference.

## Prompt

```text
Use case: historical-scene
Asset type: game environment concept art, 16:9 desktop background plate, no UI
Primary request:
Generate a playable 3/4 orthographic/isomorphic illustrated rural Irish background plate from the historic map crop and cleaned physical-linework control. The target look is the original illustrated parish notebook sample: fine hand ink, loose watercolor wash, granular paper texture, muddy roads, rough limewashed cottages, varied vegetation, human-readable facades and thresholds. Preserve map-derived topology as the source of truth, but do not turn every map line into a wall.

Input images and authority order:
Image 1: original historic map crop. Primary source evidence for layout, roads/lanes, roof marks, enclosures, tree/scrub marks, and field divisions. Top of this image is north.
Image 2: cleaned no-admin map crop. Physical-linework control. Long dotted/pecked administrative or survey-style dot chains were suppressed. Any soft grey erased seam, pale diagonal smear, or faint scar in Image 2 is a deletion artifact, not terrain.
Image 3: oblique warped cleaned control. Camera/pitch cue only for strict 3/4 orthographic/isomorphic perspective. Do not copy its paper border, blank areas, warp geometry, or erased scars.
Image 4: original illustrated parish notebook sample. Use only for overall art direction: loose ink/watercolor density, lower playable camera feel, cluttered natural brushwork, facade scale, muddy road texture, and paper atmosphere. Do not copy its UI, labels, people, church, graveyard, river, bridge, shop, signs, carts, animals, named places, or composition.
Image 5: cleaned slate-roof single-house style reference. Use for rural limewashed facade, dark timber doorway, readable threshold, slate roof texture, hand ink, and low-camera building scale.
Image 6: cleaned thatched/no-chimney single-house style reference. Use for possible thatch material, rough eaves, dark timber doorway, and no-chimney discipline.
Image 7: tree/field watercolor style reference. Use for uneven vegetation, soft hedges, grassy banks, scrub, field texture, and atmospheric watercolor only. Do not copy animals, modern composition, or any non-map content.

Map interpretation policy:
The original map crop controls the actual ground plan. The cleaned no-admin crop controls which dotted/pecked/survey linework should disappear as physical terrain. The map-reader note below is soft evidence only; the images outrank it.
Ignore printed labels, large letters, numerals, survey text, stains, paper dots, and map typography.

Data-derived soft map-reader note:
The crop shows a small rural lane network with broad pale roads or lanes crossing the lower half and linking toward the center-right. A compact central building group stands by the road frontage, including one larger dark hatched footprint and several smaller detached structures that are probably outbuildings. A second enclosed compound in the upper center-left contains two or three small dark rectangular footprints, possibly buildings or outbuildings. The center-right contains a regular enclosed planted area resembling a garden, orchard, nursery, or formal yard, while denser tree or scrub symbols gather toward the northeast and scattered tree marks appear across adjacent fields. Thin solid lines appear to be field, yard, wall, hedge, ditch, or parcel edges. Dotted and pecked lines appear more likely to be administrative or survey boundaries than physical features. No clear church, shop, water, or bridge evidence is visible.

Boundary hierarchy, extremely important:
Tier 0: suppressed dotted/pecked/admin/survey linework. Do not render it at all. No wall, hedge, fence, ditch, path, crop row, tree row, road, ridge, shadow, color seam, or decorative vegetation may trace it.
Tier 1: broad pale corridors with road-like width. Render these as muddy unfenced rural lanes/roads with irregular grassy edges, wheel ruts, stones, puddle marks, and soft shoulders. Do not automatically wall both sides of a road.
Tier 2: strong enclosed yards/gardens/compounds shown by clear continuous outlines around buildings or planted enclosures. These may have low rough stone walls, hedges, banks, gates, or mixed wall/hedge edges, but keep them low, broken, irregular, and human-scale.
Tier 3: ordinary thin single field/parcel lines. These are uncertain. Render most of them as subtle soft boundary cues: faint grass color changes, shallow ditches, broken hedge clumps, overgrown banks, low scrub, slight crop/grass texture changes, or nothing visible at all. Do not render ordinary thin lines as continuous stone walls by default.
Tier 4: internal garden/planting strokes. Render as planting beds, orchard texture, shrubs, and irregular cultivated patches only inside the planted enclosure; do not turn them into roads or walls.

Wall restraint:
This scene should not look like a stone-wall diagram. Limit stone walls to the clearest enclosed compounds and selected short segments where the map strongly supports an enclosure. Leave open field edges soft and irregular. Avoid a chessboard of continuous walls. Avoid long, straight, evenly built walls unless a strong compound/garden outline demands them. Let many boundaries fade into grass, hedge, ditch, or field texture.

Perspective and composition:
Keep north-up: features at the top of the source remain toward the top/north of the final plate; bottom remains south. Do not rotate the ground plan.
Use strict 3/4 orthographic/isomorphic game perspective. Not top-down, not survey-board, not aerial landscape. Building facades, doors, thresholds, road edges, and low boundary height must be readable for 2D character navigation.
Use a wide 16:9 plate. Cover a local playable area at a closer scale than a survey map. It is acceptable for distant crop context to fall off-frame or become understated to preserve facade size and notebook texture.
Preserve major topology: the broad lower road, central road/building frontage, upper enclosed compound, center-right planted enclosure, and northeast tree/scrub mass.

Physical scene requirements:
Render central buildings as early-19th-century rural Irish cottages/farm buildings around road frontage and working yard. The larger central footprint should read as the primary building; nearby small footprints should read as subordinate sheds/byres/barns/outbuildings.
Render the upper compound small and understated, with buildings only where dark roof marks appear.
Render the center-right planted enclosure as a garden/orchard/nursery/formal yard with internal vegetation and bed texture. Its boundary can be a mixed low wall/hedge/bank if clear, but avoid making every internal row a wall.
Render scattered map tree symbols as irregular trees, scrub, orchard remnants, or hedgerow vegetation. Keep them irregular and varied.
Roads should connect logically. Do not create extra pretty footpaths. Do not dead-end a road at an erased survey line. Do not route paths through buildings or walls without gates/openings.

Doors and buildings:
Every visible walkable house/cottage/outbuilding must have a readable human-usable doorway on a visible facade, with a small threshold connected to yard or road. This includes foreground, edge, and small outbuildings if they read as enterable. Do not mistake a window shadow for a door.
No building may sit in the middle of a road. Buildings must respect the map-derived roof footprints and road frontage.

Hard negative constraints:
Do not render any physical trace along suppressed dotted/pecked/admin boundaries from Image 2.
Do not invent water, rivers, streams, ponds, bridges, church, chapel, graveyard, shopfront, market square, signposts, labels, UI, map text, people, animals, carts, barrels, smoke, fog, or weather effects.
Do not copy any semantic object from the full notebook style sample. It is style only.
Do not add random chimneys. Do not add chimney-like stacks, roof nubs, vents, pipes, capstones, wall chimneys, isolated vertical posts, or protrusions embedded in walls or roofs. Many period thatched cottages had no chimneys; base plates must have no smoke and preferably no visible chimneys.
Do not make roofs overly regular, gardens overly grid-like, walls overly perfect, or field boundaries too diagrammatic.
Do not render as photorealism, 3D, clean vector map, fantasy village, toy miniature, mobile-game tile, or neat architectural model.

Style:
Original illustrated parish notebook environment art: sepia/dark ink outlines, sketchy crosshatching, loose watercolor washes, visible paper grain, muted greens/ochres/stone greys, muddy road whites and browns, rough vegetation blobs, uneven brushwork, imperfect hand-painted edges, lived-in rural texture. Rich and inspectable at game scale, but still a static background plate with no UI or animated effects.

Output:
One clean 16:9 illustrated isomorphic background plate, no UI. This is a direct control experiment: no prior generated plate is an edit target. The success criterion is better original-notebook art feel and reduced stone-wall over-materialization while preserving the major map topology and the deleted-admin-boundary improvement from Cycle AM.
```
