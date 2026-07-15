# Grove OS Map Crop - Reference-Pack Annotation Report

Output plate: `docs/graphics-v2/pipeline-experiments/idea-ca4n-grove-ca4n-reference-pack-annotation.png`

Scope: clean-context symbol annotation from the supplied OS 6-inch map-key reference, the supplied symbol examples/contrast sheet, and the supplied Grove crop only. Marker numbers are class IDs; repeated markers show repeated visible instances of the same class.

| Marker | Class                                   | Confidence                                                                           | Uncertainty                                                                                                                    |
| ------ | --------------------------------------- | ------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------ |
| 1      | Roofed structures / built footprints    | High for dark or hatched rectangular footprints; medium for the smallest yard marks. | Some very small rectangular marks may be yard features rather than roofed structures.                                          |
| 2      | Roads / lanes with paired solid edges   | High                                                                                 | Linework supports a physical corridor; surface type is not specified by the crop.                                              |
| 13     | Road plus dotted admin-boundary overlap | Medium-high                                                                          | Physical corridor is supported by solid edges; the dotted component is map/survey information, not a separate fence by itself. |
| 3      | Unfenced path / track candidate         | Medium-low                                                                           | Only marked where faint paired route edges appear; could instead be boundary/track-adjacent linework.                          |
| 4      | Dotted admin / survey boundary          | High                                                                                 | Interpreted as administrative/survey linework unless corroborated by physical edges or symbols.                                |
| 5      | Single solid enclosure / field boundary | High                                                                                 | The symbol marks a boundary/enclosure line, not necessarily a wall, hedge, ditch, or road.                                     |
| 6      | Double solid corridor / paired edge     | Medium                                                                               | Could be a minor lane, bank, hedge, ditch pair, or yard edge depending on local context.                                       |
| 7      | Deciduous tree symbols                  | High                                                                                 | Individual tree marks are representative; not every visible deciduous symbol is numbered.                                      |
| 8      | Coniferous tree symbols                 | High                                                                                 | Fir-shaped marks are clear; nearby puffy tree marks remain class 7.                                                            |
| 9      | Planted / mixed enclosure               | High                                                                                 | Internal texture and trees support planted enclosure/orchard/garden, not a built courtyard.                                    |
| 10     | Rough vegetation / ditch / scrub block  | Medium                                                                               | Irregular southern strip could include yard clutter or ditch vegetation; marked only where texture is clustered.               |
| 11     | Printed map text                        | High                                                                                 | Place-name printing only; not evidence of an in-world sign.                                                                    |
| 12     | Open fields / ordinary ground           | High                                                                                 | Open stippled background has no special symbol beyond ordinary field/ground in the marked spots.                               |

## Placement Notes

- Marker 1 is repeated on each likely roofed footprint: the dark/hatched rectangular marks around the Grove label, the roadside rectangular building, the smaller upper garden-side mark, and the small yard structures. The smallest marks are intentionally treated as lower-confidence roofed footprints.
- Marker 2 follows the visible paired solid-edge road/lane corridors, especially the diagonal lane through the right side and the branching upper-right corridor.
- Marker 13 marks the top-left solid corridor where dotted administrative/survey linework appears to overlay a physical road/corridor. The dotted component is not treated as a separate hedge, wall, or path.
- Marker 3 is limited to the faint lower-left route-like strip; this is an uncertain path/track candidate, not a confident road.
- Marker 4 is placed on bold dotted chains that read as administrative/survey boundary linework. The annotation deliberately separates these from physical corridors.
- Marker 5 samples single solid enclosure/field boundaries across the open fields and near the planted enclosure. It does not decide the material of the boundary.
- Marker 6 samples paired solid linework near the yard/lower approach where a double-edge corridor or bank/ditch/yard edge is visible but not confidently classed as a road.
- Markers 7 and 8 are repeated on representative tree symbols, with circular/puffy marks treated as deciduous and fir-shaped marks treated as coniferous.
- Marker 9 outlines the planted/mixed enclosure tightly around the textured, tree-filled rectangular enclosure north-west of the buildings.
- Marker 10 lightly tints the irregular clustered vegetation/ditch/scrub strip south and west of the yard. It is not applied to ordinary field stipple or to the planted enclosure texture.
- Marker 11 marks the printed place name "Grove" only. It is not interpreted as a sign or in-world object.
- Marker 12 samples open stippled field areas where no stronger feature symbol is visible.

## Absence Notes

No wells/springs, fords, quarries, railways, church symbols, contours, or marsh symbols are marked because the crop does not visibly support those classes.
