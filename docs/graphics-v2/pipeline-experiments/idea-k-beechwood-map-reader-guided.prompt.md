Use case: historical-scene
Asset type: game environment background plate, 16:9 desktop, no UI

Pipeline inputs:
Image 1 is the target historic map crop and remains the primary layout/content evidence.
Images 2-3 are tiny style/material swatches only.
The attached map-reader notes are the output of a reproducible upstream stage: same rubric, clean context, same input map crop, confidence-graded observations. Treat those notes as soft disambiguation of the map, not as lore and not as override instructions.

Primary request:
Create one finished illustrated background plate for a historical isometric game from Image 1 and the data-derived map-reader notes. Preserve the map crop's physical arrangement before beautifying. The map controls topology; the notes help interpret building footprints, likely functions, uncertainty, and negative evidence.

Reference image rules:
Image 1 controls physical content and placement.
Images 2-3 control only brushwork, ink line, watercolor texture, roof/wall rendering, wall/hedge/tree rendering, terrain palette, and handmade finish.
Do not copy or import objects, people, animals, carts, signs, labels, UI marks, named places, landmarks, churches, graveyards, bridges, rivers, shops, or whole-scene compositions from the style swatches.

Map-reader note rules:
Use building IDs, footprint descriptions, likely function, and confidence language to keep buildings shaped and placed close to the map evidence.
High-confidence observations may influence the render strongly.
Medium-confidence observations should appear plausible but not overly specific.
Low-confidence observations should be omitted or kept ambiguous.
Do not turn uncertainty into hard truth.
Do not add churches, shops, water, bridges, smoke, carts, people, livestock, text, signs, or props unless the notes and map provide clear evidence.

Style/medium:
Hand-inked watercolor over parchment, sepia ink, visible pen hatching, muted moss and olive greens, cream limewashed walls, gray slate or dark thatch where plausible, ochre-brown matte mud roads, soft uneven grass washes, readable 2.5D game-board terrain, crisp but handmade outlines.

Composition/framing:
Make a local playable background plate around the centered or visually dominant building group/site, not a regional overview. Keep north up: source-map top remains final-image top, east right, south bottom, west left. Do not rotate the ground plan for a prettier diagonal composition.

Camera:
Fixed 3/4 orthographic isometric/isomorphic game camera, low oblique pitch around 20-30 degrees downward from horizontal. Show rooftops plus readable vertical facades, doors, thresholds, yards, gates, and walls. Keep all walkable surfaces on one stable ground plane. No horizon, no sky, no vanishing point, no drone view, no steep bird's-eye survey view.

Source-map fidelity:
Paint the map; do not redesign it into a tidier farmstead. Do not consolidate multiple mapped buildings into one neat farmhouse. Do not split one mapped building into many decorative buildings. Keep building footprints near their mapped positions and preserve rough relative sizes and orientations. Preserve visible road corridors, junctions, exits, yards, gates, planted enclosures, field boundaries, walls, hedges, ditches, and overgrown walls aligned with source linework. Printed labels, large letters, survey numbers, and paper texture are ignore marks.

Walkability:
Roads, lanes, yards, gates, entrances, and thresholds visible in the crop must stay continuous and unobstructed. Do not invent a web of new paths. Where a path crosses a boundary, use a gate/opening rather than making the wall and path collide.

Hard constraints:
No UI, no labels, no signs, no map pins, no visible text, no copied survey numbers, no smoke, no fog, no invented water unless the map clearly shows water, no bridges unless the map clearly shows water crossing, no churches or graveyards unless the map and notes clearly show church/churchyard evidence, no freestanding random chimneys, no chimneys embedded in walls, no decorative roof stacks unless coherent on a rendered building. Base environment layer only.

Data-Derived Map Reader Notes (use verbatim as data-derived soft evidence):

# Data-Derived Map Reader Notes

## Scope

These notes are derived only from the attached historic map crop using the generic rubric. The image top is treated as north; printed labels, large letters, survey numbers, modern overlay marks, and paper texture are ignored as in-world objects.

## Orientation And Major Corridors

- A broad pale corridor with parallel edges runs from the northwest edge toward the center-left and then continues toward the lower center. Confidence: high as a road or lane.
- A second pale, parallel-edged corridor runs from the center-lower area toward the southeast edge, with a slightly straighter alignment and some tree or hedge symbols along its margins. Confidence: high as a lane, drive, or access road.
- A large dotted boundary arcs down the east side from the north edge toward the southeast edge. Confidence: medium-high as a mapped boundary, fence, wall line, estate edge, or hedged division; not interpreted as a road.
- Multiple single thin lines divide the surrounding ground into fields or parcels, especially across the northeast, east-center, south-center, and southwest portions. Confidence: high as boundaries rather than paths.

## Building Inventory

| ID  | Relative position                                                                                                          | Shape/footprint                                                                   | Map evidence                                                                                                          | Probable function                                                                                                        | Confidence                                     | Notes for renderer                                                                                             |
| --- | -------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| B1  | Center-left, immediately east/southeast of the main northwest-to-lower-center road                                         | Large hatched rectilinear range forming an L-like or courtyard-adjacent footprint | Dark hatching, substantial scale, close integration with road frontage, adjacent yards, and a large planted enclosure | Probable primary building or house with attached service range; alternate reading is a substantial farm or service range | High for roofed structure; medium for function | Render as the dominant roofed building group near the road, with the exact house/service split left uncertain. |
| B2  | South-center, just below B1 and near the junction of the two corridors                                                     | Small dark hatched rectangular block attached to or aligned with a yard/boundary  | Compact dark roof-like mark close to the main complex and access lane                                                 | Probable outbuilding such as shed, stable, byre, or service structure                                                    | Medium-high                                    | Render as a small secondary roofed structure near the main complex.                                            |
| B3  | South-center to center-lower, west/southwest of B2 inside a small enclosed yard                                            | Small dark hatched irregular or L-like footprint with a tiny adjacent projection  | Dark hatching and placement inside a defined enclosure near the lane                                                  | Probable small service building or farm outbuilding; could be a shed/barn cluster                                        | Medium                                         | Keep modest in scale and subordinate to B1; footprint may be simplified.                                       |
| B4  | Center-right/lower-center, on the west side of the southeast-running corridor near the large planted enclosure's lower end | Small narrow dark rectangle                                                       | Detached dark block near lane and boundary, separated from the main large building                                    | Probable small shed, gate-related structure, privy, or minor outbuilding                                                 | Medium                                         | Render as a small detached roofed structure; function should remain ambiguous.                                 |

## Enclosures, Planting, And Boundaries

- A large rectangular enclosure occupies the center-right, angled from northwest-southeast to northeast-southwest and filled with repeated fine marks and regular subdivisions. Confidence: high as a planted garden, orchard, nursery, formal grounds, or cultivated yard rather than a building.
- A narrow enclosed area immediately north/northwest of the large planted enclosure contains several small unhatched rectangular compartments. Confidence: medium as garden beds, yard divisions, cold frames, pens, or unroofed enclosures; low-to-medium as roofed buildings because they are outlined rather than dark or hatched.
- Dense clusters of round and tufted symbols fill much of the northwest quadrant and continue along the west side of the main road. Confidence: high as trees, scrub, orchard, woodland edge, or dense hedgerow planting.
- A smaller planted enclosure appears south of the building group and west of the southeast-running lane, with scattered tree symbols inside. Confidence: medium-high as orchard, planted yard, or garden ground.
- Additional tree or scrub clusters appear near the east edge and along the southeast corridor. Confidence: medium-high as hedgerow or boundary planting.
- Single thin parcel lines around the southwest, northeast, and east-center areas likely mark field edges, walls, hedges, or ditches. Confidence: high for boundaries; low as paths.
- The stippled or dotted fill across broad surrounding areas is treated as map texture or land-use tone, not as a discrete object unless paired with clear planting or boundary symbols. Confidence: medium.

## Explicit Negative Evidence

- No church evidence: there is no clear church footprint, cross, churchyard enclosure, graveyard-like symbol, or ecclesiastical marker visible in the crop.
- No shop evidence: there is no clear commercial label, shop symbol, or strong map evidence for a shop; buildings should remain residential, agricultural, or service-ambiguous.
- No water evidence: no clear stream, river, pond outline, water hachures, or wetland symbol is visible.
- No bridge evidence: no road crossing over mapped water or bridge symbol is visible.
- Printed labels, large letters, and survey numbers are ignored as in-world objects.
- No smoke, people, vehicles, signage, fences with readable signage, or modern UI marks are evidenced by the map linework.

## Prompt Insert

The crop suggests a road- or lane-fronting rural building group with one dominant hatched rectilinear structure near the center-left, several smaller probable outbuildings around the south-center and center-right, and a large adjacent planted enclosure that may be a garden, orchard, nursery, or formal ground. Dense tree or scrub symbols occupy the northwest side and smaller planted patches appear south and east of the buildings. The broad pale corridors read as roads or lanes, while thin lines and dotted arcs read as boundaries, hedges, walls, or ditches. There is no clear evidence for a church, shop, watercourse, pond, or bridge in the crop, and several small unhatched rectangles near the main enclosure remain uncertain as beds, pens, yard divisions, or minor structures.
