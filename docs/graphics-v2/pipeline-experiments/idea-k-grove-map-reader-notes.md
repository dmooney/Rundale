# Data-Derived Map Reader Notes

## Scope

These notes are derived only from the attached historic map crop using the generic rubric. The image top is treated as north, printed labels and survey-style marks are ignored as in-world objects, and all feature identifications are confidence-graded interpretations of the visible linework, tones, and symbols.

## Orientation And Major Corridors

- Broad pale corridor along the northwest edge, running roughly southwest-northeast and partly cropped: probable road or lane, medium confidence. Its open width and parallel/dotted edging make it more road-like than a simple boundary, but only a short clipped portion is visible.
- Broad pale corridor in the northeast quadrant, running from near the north edge down toward the east edge with a bend or junction near the upper-right: probable road or lane, high confidence. It has parallel edges and enough width to read as a route rather than a boundary.
- Pale open lane or road-frontage space through the center-left and lower-center, passing south of the main building group and bending toward the road in the northeast quadrant: probable lane, driveway, or road-side yard edge, medium-high confidence. Some edges are faint and partly masked by tree symbols and printed text.
- Thin straight and angled lines in the northeast, east, south, and lower-center: probable field, plot, wall, hedge, or ditch boundaries, medium-high confidence. These are not interpreted as roads unless paired with open width.
- Dotted or tree-lined boundary in the lower-left to lower-center area: probable hedge, planted boundary, wall, or ditch line, medium confidence. It may border a lane or field edge but is not enough by itself to classify as a path.

## Building Inventory

| ID  | Relative position                                                           | Shape/footprint                                                         | Map evidence                                                                            | Probable function                                                                        | Confidence  | Notes for renderer                                                                                                    |
| --- | --------------------------------------------------------------------------- | ----------------------------------------------------------------------- | --------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------- | ----------- | --------------------------------------------------------------------------------------------------------------------- |
| B1  | Center-left, just north of the lower road-frontage space                    | Low horizontal dark rectangle, slightly hatched or filled               | Dark roof-like mark integrated with yard/garden frontage and adjacent planted enclosure | Probable primary house or substantial roadside building                                  | Medium-high | Render as a modest rectangular roofed structure facing or close to the lane; exact doorway/orientation is uncertain.  |
| B2  | Center-left, immediately west or southwest of B1                            | Small pale rectangular block with darker outline                        | Detached small rectangle beside the main yard cluster                                   | Probable small outbuilding, shed, byre, privy, or yard structure                         | Medium      | Keep smaller and secondary to B1; function should remain generic.                                                     |
| B3  | Center, south of the planted enclosure and east of B1                       | Narrow dark horizontal rectangle                                        | Dark solid/hatched footprint along the yard edge                                        | Probable outbuilding or secondary roofed structure                                       | Medium      | Render as a small detached rectangular farm or service building near the lane.                                        |
| B4  | Center, immediately east of B3                                              | Small dark rectangular extension or adjoining block                     | Separate or joined dark mark adjacent to B3                                             | Probable attached shed, extension, or second small outbuilding                           | Medium-low  | Could be read as part of B3; renderer may combine with B3 if a simpler building group is needed.                      |
| B5  | Center-right, east of the planted enclosure and north of the lane bend      | Tall north-south rectangle with dark/hatched edges and lighter interior | Large rectilinear footprint within or beside a yard, larger than nearby sheds           | Probable barn, stable, byre, walled yard with roofed side, or secondary service building | Medium      | Treat as a substantial ancillary structure; the light interior leaves open whether it is fully roofed or partly open. |
| B6  | North-center-left, on or just above the north edge of the planted enclosure | Tiny dark horizontal rectangle                                          | Small detached dark block near garden/orchard enclosure                                 | Probable small shed, hut, or garden/service outbuilding                                  | Medium-low  | Render as a minor structure only if small details are being included; it should not dominate the scene.               |
| B7  | North edge, slightly left of center, partly cropped                         | Small clipped dark hatched shape                                        | Dark rectilinear mark cut off by the crop edge near a corridor                          | Possible building fragment or non-building map mark                                      | Low         | Include only if representing crop-edge context; keep ambiguous and partial.                                           |

## Enclosures, Planting, And Boundaries

- Large enclosed area in the center-left, north of B1 and B3: likely garden, orchard, nursery, planted yard, or formal ground, high confidence. It is subdivided by straight internal lines and filled with repeated small marks and tree-like symbols.
- Smaller rectilinear subdivisions inside the center-left enclosure: likely garden beds, orchard rows, or planted plots, medium-high confidence. The internal linework is regular, but the exact planting type is uncertain.
- Cluster of round/tree symbols around the south side of the building group and along the road-frontage space: likely trees, scrub, small orchard trees, or hedgerow planting, high confidence.
- Sparse round/tree symbols along the northeast road/lane and east-side boundary: likely roadside trees or hedge planting, medium-high confidence.
- Lower-left line of tree symbols and darker dotted edging: likely hedgerow, planted boundary, or field-edge trees, medium confidence. Some conifer-like symbols may represent distinct trees rather than buildings.
- Lower-center vertical dotted line extending downward from the road-frontage area: likely boundary, hedge, wall, ditch, or planted plot edge, medium confidence.
- Right-center near-vertical thin boundary with scattered tree symbols: likely field boundary or hedgerow, medium-high confidence. It is too thin and isolated to treat as a road.
- Northeast and southeast angled parcel lines: likely field or plot boundaries, medium-high confidence.
- Open pale fields across the north, east, and south portions of the crop: likely agricultural or open ground, medium confidence. The crop does not provide enough evidence to distinguish pasture, tillage, or managed estate ground.

## Explicit Negative Evidence

- No church evidence: there is no clear church footprint, cross, churchyard enclosure, graveyard-like symbol, or ecclesiastical label evidence in the crop.
- No shop evidence: there is no clear commercial label, shop symbol, market frontage, or strong map evidence to classify any building as a shop.
- No water evidence: there is no clear stream, river, pond, water hachure, wetland symbol, or watercourse linework.
- No bridge evidence: there is no clear crossing structure over water or a distinct bridge symbol.
- Printed text and large lettering near the lower-center building group are ignored as labels, not treated as in-world features.
- Survey numbers, paper texture, stipple, and modern overlay/UI marks are not treated as buildings, paths, smoke, people, vehicles, or objects.
- No smoke, fire, people, carts, animals, or active-use indicators are visible as reliable map evidence.
- Single thin lines are treated as boundaries unless paired with road-like open width; ambiguous thin marks should not be upgraded into paths without other evidence.

## Prompt Insert

The map crop suggests a small rural building cluster beside a pale lane or road-frontage space, with one probable primary rectangular structure, several smaller probable outbuildings, and a larger ambiguous ancillary rectangle that may be a barn, stable, byre, or partly open yard structure. Immediately north and west of the buildings is a rectilinear enclosed planted area with internal subdivisions, possibly garden beds, orchard, nursery, or formal grounds. Tree and hedge symbols appear around the yard, along nearby boundaries, and beside the broader lane in the northeast, while the surrounding open areas read as fields or plots. There is no clear evidence for a church, shop, watercourse, pond, or bridge in this crop, and all printed lettering is treated as non-world map annotation.
