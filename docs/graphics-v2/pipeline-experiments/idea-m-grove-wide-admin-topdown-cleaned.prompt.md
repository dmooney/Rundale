Use only the three attached images in this current message and the inserted map-reader notes below. Do not use previous generated images, prior experiment prompts, or any previous conversation context.

Input image paths for record:
- Image 1 target historic map crop: /Users/dmooney/Rundale/docs/graphics-v2/pipeline-experiments/map-crop-grove-wide-admin-boundary-test.png
- Image 2 style/material swatch only: /Users/dmooney/Rundale/docs/graphics-v2/style-crops/illustrated-style-field-wall-no-animals.png
- Image 3 style/material swatch only: /Users/dmooney/Rundale/docs/graphics-v2/style-crops/illustrated-style-wall-roof-no-props.png

PROMPT:
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

INSERTED MAP-READER NOTES:
# Data-Derived Map Reader Notes

## Scope
These notes are derived only from the attached historic map crop using the generic rubric. The image top is treated as north, printed labels and paper texture are ignored, and uncertain marks are described as probabilities rather than fixed facts.

## Orientation And Major Corridors
- West and northwest curving corridor: broad pale lane or road entering from the west edge, bending through the northwest quadrant, and continuing toward the north edge. Parallel/open edges and roadside planting make this a high-confidence road or lane.
- South-west to center-left corridor: broad pale lane or road running along the lower-left edge toward the central building group. It is lined with frequent tree symbols and has enough width to read as a physical route, high confidence.
- Northeast to center-right corridor: broad pale diagonal lane or road descending from the upper center/right toward the central-right area, with parallel/open edges and tree symbols along portions of it. High confidence as a road or lane.
- Short center-left approach: a faint curving connection from the lower-left lane toward the central yard/building frontage may be an entrance track or yard approach. Medium confidence because it is partly obscured by trees and yard marks.
- Single thin field and enclosure lines in the north, northeast, east, and southeast are more likely walls, hedges, ditches, or plot boundaries than roads. Medium confidence for boundary function, low confidence for exact material.
- Dotted or pecked lines that cut across open ground, especially the north-south dotted line near the lower center, are ambiguous and may be administrative or survey boundaries. Low confidence as physical features; they should not be treated as continuous roads, hedges, walls, fences, ditches, or planted rows without separate corroborating marks.

## Building Inventory
| ID | Relative position | Shape/footprint | Map evidence | Probable function | Confidence | Notes for renderer |
| --- | --- | --- | --- | --- | --- | --- |
| B1 | West edge, upper-left | Partial narrow horizontal rectangle | Dark hatched rectangle clipped by the crop edge | Building fragment or small outbuilding | Medium-low | Only the visible portion should be represented; footprint continues beyond crop or is incomplete. |
| B2 | West edge, center-left beside the curving road | Partial angled or compact roofed footprint | Dark hatched form close to the road edge, partly clipped | Small roadside building or outbuilding | Medium-low | Keep small and partial; relationship to any unseen structure outside the crop is uncertain. |
| B3 | Northwest quadrant beside the curving lane | Irregular angled/L-like footprint | Dark hatched roof shape integrated with road frontage | House, farm building, or roadside outbuilding | Medium | Larger and more complex than nearby small marks, but partly softened by scan blur. |
| B4 | Just north of the central planted enclosure | Small narrow horizontal rectangle | Small dark detached rectangle aligned near the enclosure edge | Shed, small barn, privy, or minor outbuilding | Medium | Detached and secondary; scale should remain modest. |
| B5 | Center-left at the south edge of the planted enclosure | Compact irregular block near the yard entrance | Dark roofed marks clustered at enclosure/yard frontage | Outbuilding or ancillary farm structure | Medium | It sits within a busy yard edge; avoid over-enlarging because adjacent marks may include walls, trees, or gates. |
| B6 | Center-south, along the main yard/frontage | Long horizontal hatched rectangle | Clear dark roofed rectangle integrated with yard and planted enclosure | Probable primary house or main farm building | High | Treat as one of the principal roofed structures in the cluster. |
| B7 | Center-south/east, just east of B6 | Shorter horizontal hatched rectangle | Separate dark roofed rectangle near yard frontage | Barn, stable, byre, or secondary domestic/farm building | High | Distinct from B6; leave a small yard gap or connection depending on final composition. |
| B8 | Center-right beside the diagonal lane | Tall narrow north-south rectangle | Clear rectangular roofed/outlined form adjacent to lane and yard | Barn, stable, cart shed, or larger outbuilding | High | Oriented north-south; should read as a substantial secondary structure rather than a boundary. |

## Enclosures, Planting, And Boundaries
- Central planted enclosure: a rectangular enclosed area in the center-left/center, subdivided into regular beds and filled with repeated planting marks. High confidence as a garden, orchard, nursery, or formal planted yard.
- Central yard: open pale space south and east of the planted enclosure, bounded by buildings, trees, and lane edges. Medium-high confidence as a working yard or forecourt.
- Tree clusters around the lower-left lane: many repeated round and conifer-like symbols line both sides of the road. High confidence as roadside trees or mixed hedgerow planting; individual symbols can be read as trees rather than a continuous wall.
- Tree symbols along the northeast/center-right lane: scattered tree marks follow the broad diagonal corridor. High confidence for roadside or boundary planting, medium confidence for whether they form a formal avenue.
- Southeast and lower-center field boundary planting: scattered tree symbols sit along or near thin boundary lines. Medium confidence as hedgerow or field-edge trees, but the line itself may be a hedge, wall, ditch, or survey boundary depending on local context.
- North and northeast field polygons: thin angular lines define large enclosed parcels. Medium confidence as field or estate boundaries; low confidence for material.
- Dotted/pecked north-south line near the lower center: likely administrative, survey, parcel, or otherwise ambiguous unless separately supported. It cuts through open ground and should not be rendered as a continuous physical feature.
- Other isolated dotted or pecked marks should be treated cautiously. Where dots coincide with a broad lane edge and roadside trees, they may support a road-edge or planted boundary reading; where they stand alone, they remain non-physical or ambiguous.

## Explicit Negative Evidence
- No church evidence: there is no clear church footprint, churchyard, cross, graveyard-like enclosure, or ecclesiastical symbol in the crop.
- No shop evidence: there is no commercial label, shop symbol, or other strong map evidence for a shop.
- No water evidence: there is no clear stream, river, pond, water hachure, bridge, or water-crossing mark.
- No bridge evidence: no road crossing over water or bridge symbol is visible.
- Printed labels and large map text are not in-world objects and should not determine building function.
- No UI or modern interface marks are visible as in-world features.
- No smoke, fire, or active industrial plume evidence is visible.
- Administrative or survey boundary rendering should be avoided for dotted/pecked/dashed lines that lack independent physical cues; such lines should not become continuous bushes, hedges, walls, fences, ditches, roads, paths, or tree rows.

## Prompt Insert
The crop suggests a rural building cluster around a planted rectangular enclosure, with one or two probable primary roofed structures, several smaller outbuildings, an open yard, and broad lanes approaching from the west, northwest, and northeast. Tree symbols appear along the lanes and some field edges, while thin parcel lines define surrounding fields. Several dotted or pecked lines appear ambiguous or administrative and do not reliably indicate physical hedges, paths, walls, or fences; uncertain marks remain secondary and approximate rather than definitive.
