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
