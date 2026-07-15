# Cycle AM - Kilteevan AJ2 Cleaned Control Direct

Mode: built-in `image_gen` tool.

## Input Roles

1. `/Users/dmooney/Rundale/docs/graphics-v2/pipeline-experiments/idea-ah-kilteevan-z17-map-crop.png` - original historic map crop; primary source for layout, roads/lanes, buildings, enclosures, tree/scrub marks, and field divisions; north is image top.
2. `/Users/dmooney/Rundale/docs/graphics-v2/pipeline-experiments/idea-aj2-kilteevan-dot-suppressed-no-admin-map-crop.png` - cleaned no-admin map crop; physical-linework control; erased seams/scars are deletion artifacts, not terrain.
3. `/Users/dmooney/Rundale/docs/graphics-v2/pipeline-experiments/idea-aj2-kilteevan-dot-suppressed-no-admin-oblique-raw-warp.png` - oblique warped cleaned control; camera/pitch cue only.
4. `/Users/dmooney/Rundale/docs/graphics-v2/style-crops/illustrated-style-low-camera-slate-single-house-door-clean.png` - slate-roof low-camera style reference for ink, walls, doors, thresholds, roof texture, watercolor grain, and facade scale.
5. `/Users/dmooney/Rundale/docs/graphics-v2/style-crops/illustrated-style-low-camera-thatched-single-house-door-clean.png` - thatched/no-chimney low-camera style reference for thatch, dark timber door styling, rough eaves, and no-chimney discipline.
6. `/Users/dmooney/Rundale/docs/graphics-v2/style-crops/illustrated-style-fields-walls.png` - field/wall/road material swatch for rough stone walls, muddy lanes, grass variation, and watercolor texture.

## Prompt

```text
Use case: historical-scene
Asset type: game environment concept art, 16:9 desktop background plate, no UI
Primary request:
Generate a playable 3/4 orthographic/isomorphic illustrated rural Irish background plate from the provided historic map crop and cleaned physical-linework control. The result should feel like the original illustrated parish notebook direction: hand-drawn ink linework with watercolor washes, granular paper texture, muddy roads, rough stone walls, uneven vegetation, and readable cottage/building facades. Preserve the map-derived topology as the source of truth.

Input images and authority order:
Image 1: original historic map crop. This is the primary source evidence for layout, roads/lanes, buildings, enclosures, tree/scrub marks, and field divisions. Treat the top of this image as north.
Image 2: cleaned no-admin map crop. This is a physical-linework control. Long dotted/pecked administrative or survey-style dot chains were suppressed in this image. Any soft grey erased seam or faint scar in Image 2 is a deletion artifact, not a real hedge, wall, road, ditch, tree row, or path.
Image 3: oblique warped cleaned control. Use only as a camera/pitch cue for strict 3/4 orthographic/isomorphic perspective. Do not copy its paper border, blank areas, or warp artifacts.
Image 4: slate-roof low-camera style reference. Use for hand-inked walls, readable doors/thresholds, roof texture, watercolor grain, and facade scale only.
Image 5: thatched/no-chimney low-camera style reference. Use for possible thatch material, dark timber door styling, rough eaves, and no-chimney discipline only.
Image 6: field/wall/road material swatch. Use for rough Irish stone walls, muddy lanes, grass variation, and watercolor texture only.

Map interpretation policy:
Use the original map crop for the actual scene layout. Use the cleaned no-admin crop to decide which dotted/pecked/survey linework should NOT become physical terrain. Render roads/lanes only where the map shows broad pale corridors, paired margins, or clear road-like width. Render thin single lines as modest field/yard/wall/hedge/ditch/parcel boundaries, not extra roads. Render planted/tree symbols as irregular trees, scrub, orchard-like planting, or hedgerow vegetation only where such symbols exist. Ignore printed labels, large letters, survey numbers, text fragments, paper texture, and map stains.

Data-derived soft map-reader note:
The crop shows a small rural lane network with broad pale roads or lanes crossing the lower half and linking toward the center-right. A compact central building group stands by the road frontage, including one larger dark hatched footprint and several smaller detached structures that are probably outbuildings. A second enclosed compound in the upper center-left contains two or three small dark rectangular footprints, possibly buildings or outbuildings. The center-right contains a regular enclosed planted area resembling a garden, orchard, nursery, or formal yard, while denser tree or scrub symbols gather toward the northeast and scattered tree marks appear across adjacent fields. Thin solid lines appear to be field, yard, wall, hedge, ditch, or parcel edges. Dotted and pecked lines appear more likely to be administrative or survey boundaries than physical features. No clear church, shop, water, or bridge evidence is visible.

Perspective and composition:
Keep the final image north-up: features at the top of the source map remain toward the top/north of the final plate, features at the bottom remain toward the bottom/south. Do not rotate the ground plan.
Use a strict 3/4 orthographic/isomorphic game camera, not a top-down survey map, not an aerial perspective, not a perspective landscape painting. The ground plane should be tilted enough that building facades, doors, thresholds, wall heights, and road edges are readable for 2D character navigation.
Make the output a wide 16:9 background plate. Cover a local playable area, not the entire arbitrary map crop if that would force a high survey view. It is acceptable for distant/cropped map context to fall off-frame or become understated if needed to preserve playable scale.
Keep roads, walls, enclosures, and buildings aligned to the map-derived topology. Do not add extra footpaths or desire lines just to make the scene pretty.

Physical scene requirements:
Render the broad lower-half lane/road as a muddy pale rural road with irregular edges. Render the center-right road/lane only if it follows a broad pale corridor from the map; keep it connected logically to the central yard/road network.
Render the central building group as rural early-19th-century Irish cottages/farm buildings around a working yard/road frontage. The larger central footprint should read as the primary building; smaller nearby footprints should read as subordinate outbuildings, sheds, byres, or barns.
Render the upper enclosed compound with small understated buildings/outbuildings only where the map shows dark roof marks. Do not overemphasize uncertain buildings.
Render the center-right planted enclosure as a garden/orchard/nursery/formal yard with internal planting texture, paths/beds only where the map suggests internal divisions, and enclosing walls/hedges/edges consistent with the map.
Render scattered trees/scrub irregularly. Avoid perfect rows unless the map’s repeated symbols strongly support planted rows.
Use modest stone walls, hedges, ditches, or overgrown boundaries for thin solid plot lines. Keep them narrow and secondary.

Hard negative constraints:
Do not render any physical feature along dot-chain/survey/admin boundaries that were suppressed in Image 2. No continuous stone wall, hedge, fence, ditch, track, footpath, crop row, tree row, shadow line, ridge, or decorative vegetation should trace those erased/suppressed dotted lines.
Do not restore the bold diagonal dotted boundary from the original crop as terrain. Do not trace soft erased seams from the cleaned crop.
Do not invent water, rivers, streams, ponds, bridges, church, chapel, graveyard, shopfront, market square, signposts, labels, UI, text, people, animals, carts, barrels, smoke, fog, or weather effects.
Do not add random chimneys. Do not add chimney-like stacks, roof nubs, vents, pipes, capstones, wall chimneys, isolated vertical posts, or protrusions embedded in walls or roofs. Many period thatched cottages had no chimneys; base plates must have no visible smoke and preferably no visible chimneys unless a source building absolutely requires one.
Every visible walkable house/cottage/outbuilding must have a readable human-usable doorway on a visible facade, with a small threshold connected to a yard or road. Do not leave foreground or partial buildings doorless. Do not confuse a window shadow with a door.
Do not place a building in the middle of a road/path. Do not make a road dead-end nonsensically at an erased survey line. Do not create roads that cross fences/walls without a gate or opening. Do not create disconnected bridge/river logic; there is no water here.
Do not render map labels, numerals, large letters, or printed text as in-world objects.

Style:
Illustrated parish notebook environment art. Fine sepia/dark ink outlines, loose watercolor fills, visible paper grain, rough muddy texture, hand-painted irregularity, muted greens/ochres/stone greys, small-scale rural detail. Not photorealistic, not a 3D render, not a clean vector map, not a polished mobile-game tile, not a fantasy village, not a miniature diorama.
Keep the plate static and layer-friendly: no smoke, no animated-looking effects, no UI overlays, no labels.

Output:
One clean 16:9 illustrated isomorphic background plate, no UI. Save as a project research artifact. This is a control experiment to test whether the cleaned no-admin map crop reduces false physical boundary rendering while preserving the original notebook look and map topology.
```
