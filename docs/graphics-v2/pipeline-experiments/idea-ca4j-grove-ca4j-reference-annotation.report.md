# Grove CA4J Reference Annotation Report

Output plate: `docs/graphics-v2/pipeline-experiments/idea-ca4j-grove-ca4j-reference-annotation.png`

Source images used: the attached OS 6-inch map-key reference sheet and the attached Grove map crop. The reference sheet was not modified. Existing repository annotation outputs were not used as interpretation evidence.

Marker numbers are feature classes, not unique object IDs. Repeated same-number dots mark representative visible instances on the crop.

| # | Class | Confidence | Marked evidence | Uncertainty / caution |
|---|---|---|---|---|
| 1 | Roofed structures | High for the larger hatched/dark rectangles; medium for the smallest detached rectangle | Five likely roofed footprints in and around the Grove compound, including the west/south cluster, central long footprint, vertical east footprint, and small upper detached mark | Some small roof-like marks are partly blurred; treated as likely roofed footprints because they match the reference sheet's rectangular structure convention |
| 2 | Double solid road / lane corridor | High | Paired solid-edge corridors through the upper/right road network and the main lane curving beside the compound | The OS key supports double solid corridors as road/path/edge linework; the exact road class is not inferred beyond visible paired edges |
| 3 | Road plus dotted admin overlay | Medium-high | Upper-left diagonal corridor with solid route edges and bold dotted linework | Read as a physical corridor carrying an administrative/survey dotted overlay; the dotted component should not be rendered as a hedge/wall by itself |
| 4 | Possible unfenced path / track | Low | Light paired horizontal route-like trace west of Grove, near the lower-left tree/dot line | This is the least certain class. It may be an unfenced path/track, but it overlaps dotted boundary and tree-line symbols, so the plate uses dashed-ring markers |
| 5 | Dotted admin / survey boundary | High | Bold round-dot chains along the lower-left horizontal and lower-center vertical linework | Treated as administrative/survey linework unless another physical symbol corroborates it |
| 6 | Single solid enclosure / field boundary | High | Thin single lines around fields, enclosure edges, and internal divisions | These marks show boundaries/enclosures only; they are not automatically walkable paths |
| 7 | Deciduous tree symbols | High | Puffy circular tree symbols sampled across the planted enclosure, roadside lines, and field edges | Repeated dots are representative rather than exhaustive; similar puffy marks are grouped with this class |
| 8 | Coniferous tree symbols | High | Tight triangular/spiky tree marks in the lower-left line and lower/right field edge | Kept separate from puffy deciduous marks per the reference sheet |
| 9 | Planted / mixed enclosure | Medium-high | Tight green outline over the regular enclosed planting/garden/orchard-like block west/northwest of the compound | The exact planting type is not inferred; the class is limited to regular enclosed planting with internal texture and trees |
| 10 | Rough vegetation / ditch / scrub block | Medium-low | Dashed brown outline and repeated markers over the irregular clustered vegetation/ditch-like strip south of the compound | Supported by the key's rough vegetation/ditch example, but the crop is blurred and includes tree symbols, so this remains plausible rather than certain |
| 11 | Printed map text | High | The printed `Grove` place label | Text is cartographic labeling only, not an in-world sign or object |
| 12 | Open fields / ordinary ground | High as ordinary ground; low as a positive feature | Representative stippled/open areas where no stronger symbol class is visible | This is an absence/ground category, included to avoid over-reading open stippled fields as roads, yards, or built features |

Useful absence note: no well, bridge, contour, quarry, or named non-Grove text symbol is confidently visible in this crop, so those reference-sheet classes are not annotated.
