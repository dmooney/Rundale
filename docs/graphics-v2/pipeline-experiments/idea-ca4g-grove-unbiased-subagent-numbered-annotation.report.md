# Grove OS-Key Numbered Annotation Report

Inputs used: the provided Grove map crop and the provided OS 6-inch map-key reference sheet. The reference sheet was not modified. The output PNG uses numbered halos only, with no arrows or text labels on the map.

| Marker | Interpreted feature class                | Confidence  | Uncertainty                                                                                                                                        |
| ------ | ---------------------------------------- | ----------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1      | Deciduous trees                          | High        | Puffy circular tree symbols line the road/corridor; individual tree species are not inferred.                                                      |
| 2      | Coniferous trees                         | High        | Tight triangular tree symbols are visible on the vertical boundary; nearby round symbols are separate deciduous marks.                             |
| 3      | Planted / mixed enclosure                | High        | Rectangular enclosed plot has internal texture plus tree marks; exact planting type is not distinguishable.                                        |
| 4      | Road plus dotted administrative boundary | Medium-high | Upper-left diagonal corridor has paired road edges with a dotted line inside; crop edge truncation prevents seeing the full corridor.              |
| 5      | Unfenced path / track                    | Medium      | Lower-left corridor shows light paired/dashed linework near trees; blurred/cropped marks leave some risk it is a non-physical dotted line instead. |
| 6      | Single solid boundary                    | High        | Lower-right single solid enclosure/field line is visible; not interpreted as a walkable path by itself.                                            |
| 7      | Double solid corridor                    | High        | Paired solid edges form a road/path corridor right of the building cluster; exact surface or route class is not inferred.                          |
| 8      | Rough vegetation / ditch block           | Medium      | Dense irregular tree/vegetation marks cluster along the compound edge; could be ditch-side vegetation or rough boundary planting.                  |
| 9      | Roofed structures                        | High        | Dark/hatched rectangular footprints mark roofed structures around the Grove yard; individual building uses are unknown.                            |
| 10     | Map text                                 | High        | Printed `Grove` label is map text only, not an in-world sign.                                                                                      |

No water, bog, crops, quarry, bridge, rock, slope, well, spring, or marsh features were annotated because no matching visible symbols were identified in the crop.
