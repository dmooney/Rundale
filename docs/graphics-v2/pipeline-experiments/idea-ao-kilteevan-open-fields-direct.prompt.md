# Cycle AO Kilteevan Open Fields Direct Prompt

Generated with the built-in `image_gen` tool from a clean-context graphics worker. This is a direct control experiment: no prior generated plate was used as an edit target.

## Input Roles

1. `/Users/dmooney/Rundale/docs/graphics-v2/pipeline-experiments/idea-ah-kilteevan-z17-map-crop.png` - original historic map crop; primary layout evidence for roads/lanes, roof marks, enclosures, tree/scrub marks, planted enclosure, and field/parcel divisions.
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
Generate a playable 3/4 orthographic/isomorphic illustrated rural Irish background plate from the historic map crop and cleaned no-admin control. Preserve the map-derived road/building/garden/tree topology, recover the original parish notebook ink-and-watercolor feel, and make open fields mostly open. The key correction from Cycle AN: do not turn ordinary open-field parcel lines into a stone-wall network.

Input images and authority order:
Image 1: original historic map crop. Primary layout evidence for roads/lanes, roof marks, enclosures, tree/scrub marks, planted enclosure, and field/parcel divisions. Top of this image is north.
Image 2: cleaned no-admin map crop. Physical-linework control. Long dotted/pecked administrative or survey dot chains were suppressed. Soft grey erased seams, pale diagonal smears, and faint scars in this image are deletion artifacts, not terrain.
Image 3: oblique warped cleaned control. Camera/pitch cue only for strict 3/4 orthographic/isomorphic perspective. Do not copy its blank borders, paper texture, warp artifacts, or erased diagonal scars.
Image 4: original illustrated parish notebook sample. Style only: use loose ink/watercolor density, rough muddy roads, readable facades, low playable camera feeling, varied brushwork, and paper atmosphere. Do not copy UI, labels, people, church, graveyard, river, bridge, shop, signposts, carts, animals, named places, or composition.
Image 5: cleaned slate-roof single-house style reference. Use for limewashed rural facade, dark timber doorway, threshold, hand ink, roof texture, and building scale.
Image 6: cleaned thatched/no-chimney single-house style reference. Use for possible thatch, rough eaves, dark timber doorway, and no-chimney discipline.
Image 7: tree/field watercolor style reference. Use for soft open fields, uneven grass, hedges, scrub, field texture, and watercolor vegetation only. Do not copy animals or non-map content.

Map interpretation policy:
The original map crop controls the ground plan. The cleaned no-admin crop controls which dotted/pecked/survey linework should vanish. Ignore printed labels, large letters, numerals, survey text, stains, paper dots, and typography.
The map-reader note is soft evidence only; image evidence outranks it.

Data-derived soft map-reader note:
The crop shows a small rural lane network with broad pale roads or lanes crossing the lower half and linking toward the center-right. A compact central building group stands by the road frontage, including one larger dark hatched footprint and several smaller detached structures that are probably outbuildings. A second enclosed compound in the upper center-left contains two or three small dark rectangular footprints, possibly buildings or outbuildings. The center-right contains a regular enclosed planted area resembling a garden, orchard, nursery, or formal yard, while denser tree or scrub symbols gather toward the northeast and scattered tree marks appear across adjacent fields. Thin solid lines appear to be field, yard, wall, hedge, ditch, or parcel edges. Dotted and pecked lines appear more likely to be administrative or survey boundaries than physical features. No clear church, shop, water, or bridge evidence is visible.

Open-field boundary rule, highest priority:
Open fields should remain visually open. Ordinary thin field/parcel lines are uncertain survey/cartographic boundaries, not automatic walls. Render most ordinary thin lines as no visible feature, faint grass color shifts, shallow drainage dips, broken hedge clumps, low overgrown banks, scattered scrub, or subtle field texture changes. They should be easy to overlook at first glance.
Only the clearest domestic yards, building compounds, and planted garden/orchard enclosure may receive visible boundary treatment. Even there, prefer mixed, low, broken, irregular boundaries: short stone wall fragments, gaps, hedges, earth banks, rough gate openings, or overgrown wall remnants.
Do not create a connected stone-wall network across open fields. Do not outline every field. Do not run stone walls along both sides of every road. Do not make a chessboard of walls. Do not trace long straight walls through open grass unless the map shows a strong enclosed yard/garden/compound.

Boundary hierarchy:
Tier 0: suppressed dotted/pecked/admin/survey linework from Image 2. Render nothing. No wall, hedge, fence, ditch, road, footpath, crop row, tree row, ridge, shadow, seam, or vegetation trace.
Tier 1: broad pale road corridors. Render as muddy unfenced rural lanes/roads with soft grass shoulders, wheel ruts, stones, puddle marks, and uneven edges. Roads can have occasional short wall or hedge fragments near yards/gates, but not continuous wall borders.
Tier 2: immediate domestic yards and building compounds. Use readable but broken boundaries only where needed to define yards: short low wall fragments, hedge/bank segments, open gates, gaps, and worn thresholds.
Tier 3: planted garden/orchard enclosure. Its outer edge may be clearer than field lines, but should still be a mixed low wall/hedge/bank with breaks and a gate. Internal bed lines are plants and soil texture, not walls.
Tier 4: ordinary open-field parcel lines. Mostly invisible or soft vegetation/ditch/grass texture. No continuous stone wall treatment.

Perspective and composition:
Keep north-up: top of source map remains toward the top/north of final plate; bottom remains south. Do not rotate the ground plan.
Use strict 3/4 orthographic/isomorphic game perspective. Not top-down, not survey-board, not aerial landscape. Building facades, doors, thresholds, and road edges should be readable for 2D character navigation.
Use a wide 16:9 plate at playable scale. Do not force the whole arbitrary map crop into view if that makes the camera too high. It is acceptable for distant source-map context to fall off-frame or become understated.
Preserve major topology: broad lower road, central road/building frontage, upper enclosed compound, center-right planted enclosure, and northeast tree/scrub mass.

Physical scene requirements:
Central cluster: early-19th-century rural Irish cottages/farm buildings around road frontage and working yard. The largest central roof mark should read as the primary building; smaller nearby roof marks should read as subordinate sheds, byres, barns, or outbuildings.
Upper compound: small and understated; render buildings only where dark roof marks appear.
Center-right planted enclosure: garden/orchard/nursery/formal yard with cultivated beds, shrubs, small trees, and soil/plant texture. Do not convert internal bed lines into paths or walls.
Trees/scrub: place irregular trees, scrub, orchard remnants, and hedgerow vegetation where map symbols support them. Keep variation natural and uneven.
Road logic: roads must connect plausibly. Do not add decorative footpaths. Do not dead-end roads at erased survey lines. Do not route paths through buildings or closed boundaries without gates/openings.

Doors and building readability:
Every visible walkable house/cottage/outbuilding must have a readable human-usable dark timber doorway on a visible facade, with a small threshold connected to yard or road. This includes small and edge buildings if they read as enterable. Do not mistake a window shadow for a door.
Buildings must not sit in the middle of roads. Buildings must respect map-derived roof footprints and road frontage.

Hard negative constraints:
No physical trace along suppressed dotted/pecked/admin boundaries from Image 2.
No water, rivers, streams, ponds, bridges, church, chapel, graveyard, shopfront, market square, UI, labels, map text, people, animals, carts, barrels, smoke, fog, or weather effects.
Do not copy semantic content from the full notebook sample; it is style only.
No random chimneys. No chimney-like stacks, roof nubs, vents, pipes, capstones, wall chimneys, isolated vertical posts, or protrusions embedded in roofs or walls. No visible smoke. Many period thatched cottages had no chimneys.
No overly regular roof grids, garden grids, perfect walls, continuous field outlines, fantasy styling, photorealism, 3D rendering, clean vector map, mobile-game tile look, or toy miniature look.

Style:
Original illustrated parish notebook environment art: sepia/dark ink outlines, sketchy crosshatching, loose watercolor washes, visible paper grain, muted greens/ochres/stone greys, muddy road whites and browns, rough vegetation blobs, uneven brushwork, imperfect hand-painted edges, lived-in rural texture. Rich and inspectable at game scale, but a clean static background plate with no UI or animated effects.

Output:
One clean 16:9 illustrated isomorphic background plate, no UI. This is a direct control experiment: no prior generated plate is an edit target. Success means: more open-field softness than Cycle AN, no restored deleted-admin boundary, preserved major map topology, readable doors, no chimneys/smoke, and stronger original-notebook art feel.
```
