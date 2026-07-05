# Data-Derived Map Reader Notes

## Scope

These notes are derived only from the attached historic map crop using the generic rubric. The image top is treated as north; printed labels, large letters, survey numerals, paper texture, and other map-print artifacts are not treated as in-world objects.

## Orientation And Major Corridors

- Lower-half lane or road, medium-high confidence: a broad pale corridor enters from the west-southwest edge, passes toward the central building cluster, and appears to continue or branch toward the lower center. Its open width and paired margins make it more road-like than boundary-like.
- Center-right lane or road, medium confidence: a pale corridor runs roughly from the lower center toward the center-right and north-northeast, skirting the west side of the regular planted enclosure. It may be a lane linking the central buildings to the northeast side of the crop.
- Possible small road junction or yard access at the central building cluster, medium confidence: the broad pale corridors converge near the main central roofs, suggesting road frontage or yard access rather than isolated field paths.
- Bold dotted diagonal line from near the north-center toward the southeast edge, high confidence as non-building linework and low confidence as a physical feature: it lacks independent road width, hedge/tree symbols riding the line, wall hatching, or gate relationships. It is best read as an administrative, survey, estate, townland, parish, or parcel boundary and ignored as physical terrain unless corroborated by adjacent map evidence.
- Curving dotted or pecked line across the west and center-left, medium-high confidence as survey or administrative linework: it crosses printed text and open ground without physical corroboration, so it should not be rendered as a hedge, wall, track, ditch, crop row, or tree row.
- Thin solid lines throughout the lower and central crop, medium confidence: these likely mark field, yard, garden, wall, hedge, ditch, or parcel edges. Single thin lines should not be treated as roads unless connected to broader pale corridors or supported by other physical marks.

## Building Inventory

| ID  | Relative position                                                                 | Shape/footprint                                                      | Map evidence                                                                      | Probable function                                                   | Confidence  | Notes for renderer                                                                                        |
| --- | --------------------------------------------------------------------------------- | -------------------------------------------------------------------- | --------------------------------------------------------------------------------- | ------------------------------------------------------------------- | ----------- | --------------------------------------------------------------------------------------------------------- |
| B1  | Center-left, just north of the lower-half lane                                    | Larger dark hatched rectangular or slightly irregular footprint      | Solid/hatched roof mark integrated with road frontage and nearby small structures | Probable house, farmhouse, or primary roofed building               | High        | Render as the main building of the central cluster, with uncertain exact roof orientation.                |
| B2  | Center-left, immediately south or southwest of B1                                 | Small detached dark rectangular footprint, angled                    | Separate dark roof mark close to B1 and the lane                                  | Probable shed, barn, stable, byre, or other outbuilding             | Medium-high | Keep subordinate to B1; likely part of the same yard group.                                               |
| B3  | Lower center-left, south of B1                                                    | Small detached dark rectangular footprint                            | Small roof mark set beside the lane or yard edge                                  | Probable farm outbuilding                                           | Medium      | Could be a small service building rather than a dwelling.                                                 |
| B4  | Center-left to lower center-left, southeast of B1                                 | Small dark rectangular or angular footprint                          | Detached roof mark near B2 and B3                                                 | Probable outbuilding                                                | Medium      | Treat as part of the central working-yard cluster; exact count and alignment are uncertain.               |
| B5  | Upper center-left inside a thin-lined enclosure                                   | Small dark rectangular footprint near the west side of the enclosure | Hatching/solid roof mark inside an enclosed yard or garden                        | Probable small dwelling or substantial outbuilding                  | Medium      | Associated with the upper enclosed compound; function is uncertain.                                       |
| B6  | Upper center-left inside the same enclosure as B5                                 | Narrow dark rectangular footprint                                    | Detached roof mark within the enclosure                                           | Probable outbuilding, shed, or small secondary structure            | Medium      | Should be smaller and separate from B5.                                                                   |
| B7  | Upper center-left, southeast corner of the same enclosure near a lane or boundary | Dark compact rectangular or blocky footprint                         | Roof-like dark mark integrated with enclosure edge and nearby corridor            | Probable building or outbuilding associated with the upper compound | Medium-low  | Ambiguous because it sits near crossing linework and tree symbols; avoid overemphasizing it.              |
| B8  | Far northwest to west edge, partly cut off by the crop                            | Irregular partial shaded forms                                       | Truncated dark/grey marks at the image edge                                       | Possible partial buildings or non-building map marks                | Low         | Too cropped for reliable interpretation; omit or leave ambiguous unless neighboring map context confirms. |

## Enclosures, Planting, And Boundaries

- Upper center-left enclosed compound, medium confidence: a thin-lined polygon contains several dark roof marks and scattered small round symbols. It reads as a yard, garden, small holding, or enclosed domestic/farmstead plot rather than open field alone.
- Center-right regular enclosed planted area, high confidence: a rectangular or polygonal enclosure contains repeated internal bed-like strokes and subdivisions. It is likely a garden, orchard, nursery, planted yard, or formal ground rather than a building.
- Narrow planted or textured strip north of the center-right garden, medium confidence: repeated small marks beside the lane suggest a planted margin, garden extension, scrub patch, or textured yard surface.
- Northeast quadrant tree/scrub cluster, high confidence: dense repeated round/tree symbols indicate trees, scrub, orchard, woodland edge, or hedgerow planting. Exact species and cultivation status are not knowable from the crop.
- Scattered round symbols across the fields and near boundaries, medium confidence: likely individual trees, bushes, scrub, orchard remnants, or hedgerow planting. They should remain sparse and irregular rather than crop rows.
- Lower-center triangular or tapering enclosure, medium confidence: thin lines form a long narrow plot south of the central buildings. It may be a field, paddock, roadside plot, ditch/wall-bounded enclosure, or garden extension, not a roofed structure.
- Field and plot divisions, medium confidence: many thin solid lines likely represent walls, hedges, ditches, or parcel boundaries. They should remain secondary linear features and should not be widened into roads.
- Dotted, pecked, or dot-chain boundaries, high confidence as ambiguous/non-physical unless corroborated: these should not become hedges, walls, fences, ditches, paths, tree rows, crop rows, or roads based on the crop alone.

## Explicit Negative Evidence

- No church evidence: no clear church footprint, cross, graveyard-like enclosure, churchyard symbol, or ecclesiastical map mark is visible.
- No shop evidence: no clear commercial label, shop symbol, storefront convention, or other strong commercial map evidence is visible.
- No water evidence: no clear stream, river, pond, water hachure, wetland shape, or drainage channel is visible.
- No bridge evidence: no road-over-water crossing, bridge symbol, or watercourse crossing is visible.
- Printed labels, large letters, survey numbers, and text fragments are excluded from the in-world interpretation.
- Bold dotted and pecked lines are not treated as physical objects without corroborating physical marks.
- No reliable evidence for people, animals, carts, smoke, modern UI marks, or temporary activity is visible in the crop.

## Prompt Insert

The crop shows a small rural lane network with broad pale roads or lanes crossing the lower half and linking toward the center-right. A compact central building group stands by the road frontage, including one larger dark hatched footprint and several smaller detached structures that are probably outbuildings. A second enclosed compound in the upper center-left contains two or three small dark rectangular footprints, possibly buildings or outbuildings. The center-right contains a regular enclosed planted area resembling a garden, orchard, nursery, or formal yard, while denser tree or scrub symbols gather toward the northeast and scattered tree marks appear across adjacent fields. Thin solid lines appear to be field, yard, wall, hedge, ditch, or parcel edges. Dotted and pecked lines appear more likely to be administrative or survey boundaries than physical features. No clear church, shop, water, or bridge evidence is visible.
