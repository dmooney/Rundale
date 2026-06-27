# Beechwood Admin Top-Down Cleaned Prompt

Generated with the built-in `image_gen` tool.

## Output

- Saved image: `/Users/dmooney/Rundale/docs/graphics-v2/pipeline-experiments/idea-m-beechwood-admin-topdown-cleaned.png`
- Built-in generated source: `/Users/dmooney/.codex/generated_images/019f0a54-7881-7ab2-8efa-91fe34dbecc9/ig_0a0f2cdaf9fb0275016a401685f7c88197bca527bf72932189.png`

## Input Paths

- Image 1, target historic map crop: `/Users/dmooney/Rundale/docs/graphics-v2/pipeline-experiments/map-crop-control-02.png`
- Image 2, field/wall style swatch: `/Users/dmooney/Rundale/docs/graphics-v2/style-crops/illustrated-style-field-wall-no-animals.png`
- Image 3, wall/roof style swatch: `/Users/dmooney/Rundale/docs/graphics-v2/style-crops/illustrated-style-wall-roof-no-props.png`
- Map-reader notes: `/Users/dmooney/Rundale/docs/graphics-v2/pipeline-experiments/idea-m-beechwood-admin-boundary-map-reader-notes.md`

## Exact Prompt Sent To `image_gen`

```text
Use the three attached images visible in this conversation as the only visual references. Do not use any generated images as reference.

Input image roles:
Image 1: target historic map crop and primary layout/content evidence. Preserve its topology, source-map top as north, and relative positions.
Image 2: tiny style/material swatch only for field, wall, ink, vegetation texture and muted color handling. Do not copy objects or composition.
Image 3: tiny style/material swatch only for roof, stone/limewash, ink, wall, and field texture. Do not copy objects or composition.

Critical M1 control priority:
The prominent dotted/pecked/dot-chain boundary on the northeast and east of Image 1 is administrative/survey or ambiguous non-physical. It must disappear into open field texture. Do not draw any continuous curved hedge, wall, fence, ditch, path, road, tree row, crop row, ridge, or boundary object along the right/east or northeast arc where that dotted line appears. The east and northeast fields should remain open except for physical solid field/plot lines that have independent evidence. This is the most important correction.

Use case: historical-scene
Asset type: top-down cleaned illustrated control plate, native 16:9 desktop, no UI

Pipeline inputs:
Image 1 is the target historic map crop and remains the primary layout/content evidence.
Images 2-3 are tiny style/material swatches only.
The attached map-reader notes are a reproducible upstream artifact for this same crop. Treat them as confidence-graded soft evidence, not lore and not an override of the map.

Primary request:
Create a cleaned, top-down, north-up illustrated plan plate from Image 1 and the map-reader notes. This is not the final isometric game plate. It is a layout-preserving cleaned art/control plate for a later isomorphic conversion step. Preserve the crop topology as exactly as possible while translating map symbols into readable painted terrain, roads, physical boundaries, planting, building footprints, yards, and gardens.

Camera and geometry:
Strict top-down orthographic plan view. No isometric perspective, no oblique camera, no visible facades, no horizon, no sky, no vanishing point. Keep source-map top as final-image top, east right, south bottom, west left. Do not rotate the plan. Preserve relative positions, angles, footprint proportions, road widths, junctions, enclosed planting, and physical boundaries.

Map-key administrative/survey boundary rule:
Historic OS-style map keys include dotted, pecked, dashed, or dot-chain boundaries that can mark non-physical administrative or survey divisions such as townland, parish, barony, county, estate, or parcel boundaries. These are not terrain. If a dotted/pecked/dashed line lacks independent physical evidence, it must disappear into the field texture. Do not render it as bushes, hedges, walls, fences, ditches, paths, roads, tree rows, crop rows, ridges, or any continuous in-world object. Only draw a dotted/pecked/dashed line as physical when the original map also shows corroborating physical evidence: tree/hedge symbols riding the line, paired road edges, wall/ditch hatching, enclosure-edge continuity, gate/yard relationship, or another physical map mark. If uncertain, omit the line as a physical feature.

Map-reader note rules:
Use building IDs, footprint descriptions, likely function, and confidence language to keep mapped structures separated and shaped close to the evidence.
High-confidence observations may influence the cleaned plan strongly.
Medium-confidence observations should appear plausible but generic.
Low-confidence observations should be omitted, cropped, or kept ambiguous.
Do not turn uncertainty into hard truth.
Explicit negative evidence should suppress churches, shops, water, bridges, smoke, people, livestock, carts, signs, UI, and text.
If the notes identify dotted/pecked/dashed linework as administrative, survey, or ambiguous non-physical, do not draw that line.

Top-down translation:
Buildings remain top-down roof or footprint shapes, not 3D volumes. Represent primary buildings, sheds, barns, byres, stables, walled yards, or ambiguous ancillary structures only as plan-view footprints with subtle roof/yard texture. Roads and lanes become matte ochre-brown dirt corridors. Single thin physical parcel lines become modest hedges, walls, ditches, plot boundaries, or overgrown boundaries, not extra paths. Enclosed planted areas become top-down gardens, orchards, nurseries, beds, or planted yards according to the map and notes. Tree symbols become top-down tree canopies or scrub clusters placed where the map shows them.

Native 16:9 framing:
Produce a native 16:9 plate from the source crop context. Do not add synthetic side padding, mirrored margins, blurred edge extension, cloned fields, or decorative borders. Keep the local site large enough for the final isomorphic pass, but include enough real visible source-map context toward the top/north of the crop that later tilt has content to work with. If the map crop aspect ratio differs from 16:9, frame the local area with real visible crop content; do not invent a regional overview.

Style/medium:
Hand-inked watercolor over parchment, sepia ink line, muted moss and olive greens, cream/gray stone and limewash cues, soft uneven grass washes, restrained top-down roof hatching, handmade but clean enough to serve as a control image. Use the style swatches only for texture, color, ink, wall/roof/field treatment, not for objects or composition.

Walkability/topology:
Roads, lanes, yards, gates, entrances, and thresholds visible in the crop must remain continuous and unobstructed. Do not invent a web of new paths. Where a route crosses a physical boundary, show a gap, gate, or opening rather than a collision. Do not place trees, buildings, walls, or props in the middle of roads or yard centers.

Hard constraints:
No UI, no labels, no signs, no map pins, no visible text, no copied survey numbers, no copied style-reference objects, no people, no animals, no carts, no smoke, no fog, no invented water unless the map and notes clearly show water, no bridges unless the map and notes clearly show a water crossing, no churches or graveyards unless the map and notes clearly show church/churchyard evidence, no decorative chimneys, no freestanding random chimneys, no chimneys embedded in walls, no facades, no cast 3D building shadows. Base cleaned control layer only.

Map-reader notes to insert and follow as confidence-graded soft evidence:
# Data-Derived Map Reader Notes

## Scope
These notes are derived only from the attached historic map crop using the generic rubric. The image top is treated as north, printed labels/large letters/survey numbers/paper texture are ignored as in-world objects, and uncertain marks are described with confidence rather than converted into hard constraints.

## Orientation And Major Corridors
- Main diagonal road or lane, high confidence: A broad pale corridor with parallel edges enters from the northwest edge, runs diagonally through the center-left, passes directly beside the main building group, and continues toward the south-southeast/lower-right edge. Its width and paired edges support reading it as a road/lane rather than a thin boundary.
- Roadside yard frontage, medium-high confidence: The building group sits tightly against the main corridor around the central and south-central crop, suggesting a road-facing yard or service edge. Some dark structures directly touch or nearly touch the lane edge.
- Thin single-line field or plot boundaries, medium confidence: Several single thin lines curve or angle through the southwest, south, and east portions of the crop. By rubric these are more likely walls, hedges, ditches, or plot boundaries than paths; the exact material is uncertain.
- Prominent dotted/pecked line in the northeast and east, high confidence as administrative/survey or ambiguous non-physical boundary: A dot-chain line curves from the north edge down the eastern side, cutting across open parcel areas without corroborating tree symbols, road edges, wall hatching, ditch marks, or gate/yard relationships. It should not be rendered as a continuous hedge, wall, fence, road, path, ditch, or tree row.
- Faint dashed/pecked line near center-left to south-center, medium confidence as ambiguous/non-physical: A faint broken line appears to cross open ground near the building group and lower central crop. Because it lacks independent physical cues, it should be treated as administrative/survey/ambiguous rather than drawn as an in-world continuous feature.

## Building Inventory
B1: Center-left, immediately east/southeast of the main diagonal lane. Dark hatched rectilinear block, roughly L-shaped or range-like, with a pale inner/open area adjacent to the road. Evidence: dark roof hatching and solid rectilinear outline integrated with road frontage and the central enclosed grounds. Probable function: primary house, farmhouse, or main roadside building. Confidence medium-high. Renderer note: render as the largest roofed building in the group, aligned with the lane; allow a small attached/open yard or court where the pale gap appears.
B2: South-central, just below the large central enclosed planted rectangle and close to the lane. Small dark rectangular block, detached or lightly attached to the enclosure/yard edge. Evidence: dark solid/hatched footprint near the main yard. Probable function: shed, stable, byre, small barn, or service outbuilding. Confidence medium. Renderer note: render as a small auxiliary roofed structure, lower and simpler than B1.
B3: Lower center-left/south-central, on the opposite side of the lane from the large enclosed planted area. Compact cluster of dark hatched rectilinear pieces forming an irregular small compound. Evidence: multiple dark roof-like marks grouped around a small yard/plot near the road. Probable function: farm outbuildings, barn/stable/byre range, or service yard. Confidence medium. Renderer note: render as a modest outbuilding cluster rather than a single grand house; individual pieces may be connected or very close together.
B4: Center-right/southeast of the large enclosed planted rectangle, beside or just off the lane. Very small dark rectangular block. Evidence: small dark solid footprint separated from the main cluster. Probable function: minor shed, privy, small store, or other outbuilding. Confidence low-medium. Renderer note: render only as a small secondary structure if the scene needs it; keep it subordinate and uncertain.

## Enclosures, Planting, And Boundaries
- Large central enclosed planted area, high confidence: A large pale rectangle occupies the center to center-right, outlined by a thin boundary and filled with regular repeated internal marks. It reads more like a garden, planted yard, nursery, orchard, or formal ground than a roofed building.
- Small regular northern enclosures, medium confidence: Two small outlined rectangles north of B1 and northwest of the large central planted rectangle may be garden beds, yard compartments, small pens, or non-roofed enclosures. They are not strongly marked as roofed buildings.
- Northwest and center-left tree/scrub mass, high confidence: Dense clusters of round and small tree symbols occupy much of the northwest quadrant and center-left west of the road, suggesting woodland edge, scrub, orchard-like planting, or thick hedgerow planting.
- Southern small tree/scrub clusters, medium-high confidence: Smaller clusters of tree symbols appear around the lower central building group and near the southeast edge, suggesting planted edges, orchard fragments, scrub, or hedgerow vegetation.
- Open field or parcel areas, medium confidence: Pale open areas with thin boundary lines occupy the southwest, east, and northeast portions. The printed stipple/paper texture is not treated as vegetation; only explicit tree or planting symbols are rendered as plantings.
- Curving southwest/south boundary, medium confidence: A thin curving line along the southwest and lower portions likely marks a plot, field, wall, hedge, or ditch edge. Material is uncertain and should remain visually modest.
- Eastern dotted/pecked boundary, high confidence non-physical/ambiguous: The curving dot-chain boundary on the east lacks physical support and should be omitted as a continuous terrain object. It may be useful only as invisible source-map context.
- Faint broken center-left boundary, medium confidence non-physical/ambiguous: The faint dashed/pecked line near the lower center-left has insufficient physical cues. If represented at all, it should be treated as uncertain survey information, not as a hedge, path, ditch, or fence.

## Explicit Negative Evidence
- Church: No clear church, churchyard, cross, graveyard enclosure, ecclesiastical footprint, or church label evidence appears in the crop.
- Shop: No clear commercial label, shop symbol, storefront pattern, or strong map evidence supports classifying any structure as a shop.
- Water: No clear stream, river, pond, water hachure, marsh/water edge, or watercourse evidence appears.
- Bridge: No bridge or water-crossing evidence appears.
- Text and survey marks: Printed place labels, large letters, parcel/survey numbers, and paper texture are present but are not in-world objects.
- Modern UI or overlay: No in-world UI, modern overlay object, smoke plume, vehicles, people, or animals are evidenced by the crop.
- Administrative boundary rendering: The dotted/pecked and faint dashed lines should not be converted into continuous bushes, hedges, walls, fences, paths, roads, ditches, or tree rows unless later independent evidence supports a physical feature.

## Prompt Insert
The crop shows a broad pale lane running diagonally from the northwest toward the south-southeast, passing a compact roadside group of dark hatched buildings. The most substantial roofed block sits near the center-left with several smaller probable outbuildings around the south-central yard, while a large outlined rectangular area to the center-right appears more likely to be enclosed planted ground or formal garden than a building. Dense tree or scrub symbols occupy the northwest and smaller planting clusters appear near the lower buildings and southeast edge. Thin single lines suggest field or plot boundaries with uncertain material, and a prominent dotted/pecked curving line on the east appears administrative, survey, or otherwise non-physical rather than a drawable hedge, wall, fence, road, path, or ditch. There is no clear evidence of a church, shop, watercourse, pond, or bridge.
```

## Source Map-Reader Notes Inserted

```markdown
# Data-Derived Map Reader Notes

## Scope
These notes are derived only from the attached historic map crop using the generic rubric. The image top is treated as north, printed labels/large letters/survey numbers/paper texture are ignored as in-world objects, and uncertain marks are described with confidence rather than converted into hard constraints.

## Orientation And Major Corridors
- **Main diagonal road or lane, high confidence:** A broad pale corridor with parallel edges enters from the northwest edge, runs diagonally through the center-left, passes directly beside the main building group, and continues toward the south-southeast/lower-right edge. Its width and paired edges support reading it as a road/lane rather than a thin boundary.
- **Roadside yard frontage, medium-high confidence:** The building group sits tightly against the main corridor around the central and south-central crop, suggesting a road-facing yard or service edge. Some dark structures directly touch or nearly touch the lane edge.
- **Thin single-line field or plot boundaries, medium confidence:** Several single thin lines curve or angle through the southwest, south, and east portions of the crop. By rubric these are more likely walls, hedges, ditches, or plot boundaries than paths; the exact material is uncertain.
- **Prominent dotted/pecked line in the northeast and east, high confidence as administrative/survey or ambiguous non-physical boundary:** A dot-chain line curves from the north edge down the eastern side, cutting across open parcel areas without corroborating tree symbols, road edges, wall hatching, ditch marks, or gate/yard relationships. It should not be rendered as a continuous hedge, wall, fence, road, path, ditch, or tree row.
- **Faint dashed/pecked line near center-left to south-center, medium confidence as ambiguous/non-physical:** A faint broken line appears to cross open ground near the building group and lower central crop. Because it lacks independent physical cues, it should be treated as administrative/survey/ambiguous rather than drawn as an in-world continuous feature.

## Building Inventory
| ID | Relative position | Shape/footprint | Map evidence | Probable function | Confidence | Notes for renderer |
| --- | --- | --- | --- | --- | --- | --- |
| B1 | Center-left, immediately east/southeast of the main diagonal lane | Dark hatched rectilinear block, roughly L-shaped or range-like, with a pale inner/open area adjacent to the road | Dark roof hatching and solid rectilinear outline integrated with road frontage and the central enclosed grounds | Primary house, farmhouse, or main roadside building | Medium-high | Render as the largest roofed building in the group, aligned with the lane; allow a small attached/open yard or court where the pale gap appears. |
| B2 | South-central, just below the large central enclosed planted rectangle and close to the lane | Small dark rectangular block, detached or lightly attached to the enclosure/yard edge | Dark solid/hatched footprint near the main yard | Shed, stable, byre, small barn, or service outbuilding | Medium | Render as a small auxiliary roofed structure, lower and simpler than B1. |
| B3 | Lower center-left/south-central, on the opposite side of the lane from the large enclosed planted area | Compact cluster of dark hatched rectilinear pieces forming an irregular small compound | Multiple dark roof-like marks grouped around a small yard/plot near the road | Farm outbuildings, barn/stable/byre range, or service yard | Medium | Render as a modest outbuilding cluster rather than a single grand house; individual pieces may be connected or very close together. |
| B4 | Center-right/southeast of the large enclosed planted rectangle, beside or just off the lane | Very small dark rectangular block | Small dark solid footprint separated from the main cluster | Minor shed, privy, small store, or other outbuilding | Low-medium | Render only as a small secondary structure if the scene needs it; keep it subordinate and uncertain. |

## Enclosures, Planting, And Boundaries
- **Large central enclosed planted area, high confidence:** A large pale rectangle occupies the center to center-right, outlined by a thin boundary and filled with regular repeated internal marks. It reads more like a garden, planted yard, nursery, orchard, or formal ground than a roofed building.
- **Small regular northern enclosures, medium confidence:** Two small outlined rectangles north of B1 and northwest of the large central planted rectangle may be garden beds, yard compartments, small pens, or non-roofed enclosures. They are not strongly marked as roofed buildings.
- **Northwest and center-left tree/scrub mass, high confidence:** Dense clusters of round and small tree symbols occupy much of the northwest quadrant and center-left west of the road, suggesting woodland edge, scrub, orchard-like planting, or thick hedgerow planting.
- **Southern small tree/scrub clusters, medium-high confidence:** Smaller clusters of tree symbols appear around the lower central building group and near the southeast edge, suggesting planted edges, orchard fragments, scrub, or hedgerow vegetation.
- **Open field or parcel areas, medium confidence:** Pale open areas with thin boundary lines occupy the southwest, east, and northeast portions. The printed stipple/paper texture is not treated as vegetation; only explicit tree or planting symbols are rendered as plantings.
- **Curving southwest/south boundary, medium confidence:** A thin curving line along the southwest and lower portions likely marks a plot, field, wall, hedge, or ditch edge. Material is uncertain and should remain visually modest.
- **Eastern dotted/pecked boundary, high confidence non-physical/ambiguous:** The curving dot-chain boundary on the east lacks physical support and should be omitted as a continuous terrain object. It may be useful only as invisible source-map context.
- **Faint broken center-left boundary, medium confidence non-physical/ambiguous:** The faint dashed/pecked line near the lower center-left has insufficient physical cues. If represented at all, it should be treated as uncertain survey information, not as a hedge, path, ditch, or fence.

## Explicit Negative Evidence
- **Church:** No clear church, churchyard, cross, graveyard enclosure, ecclesiastical footprint, or church label evidence appears in the crop.
- **Shop:** No clear commercial label, shop symbol, storefront pattern, or strong map evidence supports classifying any structure as a shop.
- **Water:** No clear stream, river, pond, water hachure, marsh/water edge, or watercourse evidence appears.
- **Bridge:** No bridge or water-crossing evidence appears.
- **Text and survey marks:** Printed place labels, large letters, parcel/survey numbers, and paper texture are present but are not in-world objects.
- **Modern UI or overlay:** No in-world UI, modern overlay object, smoke plume, vehicles, people, or animals are evidenced by the crop.
- **Administrative boundary rendering:** The dotted/pecked and faint dashed lines should not be converted into continuous bushes, hedges, walls, fences, paths, roads, ditches, or tree rows unless later independent evidence supports a physical feature.

## Prompt Insert
The crop shows a broad pale lane running diagonally from the northwest toward the south-southeast, passing a compact roadside group of dark hatched buildings. The most substantial roofed block sits near the center-left with several smaller probable outbuildings around the south-central yard, while a large outlined rectangular area to the center-right appears more likely to be enclosed planted ground or formal garden than a building. Dense tree or scrub symbols occupy the northwest and smaller planting clusters appear near the lower buildings and southeast edge. Thin single lines suggest field or plot boundaries with uncertain material, and a prominent dotted/pecked curving line on the east appears administrative, survey, or otherwise non-physical rather than a drawable hedge, wall, fence, road, path, or ditch. There is no clear evidence of a church, shop, watercourse, pond, or bridge.
```
