# Grove OS-map annotation report

Output plate: `docs/graphics-v2/pipeline-experiments/idea-ca4i-grove-unbiased-ca3-style-annotation.png`

Evidence used: the supplied 6-inch OS map-key reference sheet and the supplied Grove map crop only. The reference sheet was not modified.

## Marker classes

1. **Roofed structures**  
   Confidence: high to medium.  
   Marked each likely dark or hatched rectangular roof footprint individually, including the main compound roofs, the detached small rectangle north of the planted enclosure, the long east/south footprint, the larger east vertical footprint, and a partial top-edge footprint. The partial top-edge item is less certain because only part of the symbol is visible.

2. **Roads / double solid corridors**  
   Confidence: high.  
   Markers follow paired solid corridor geometry at the upper-left diagonal corridor, the main diagonal lane/corridor through the right half of the crop, and the upper-right corridor segment. These are interpreted as road/lane/corridor linework, not merely single enclosure boundaries.

3. **Dotted administrative / survey boundaries**  
   Confidence: high.  
   Marked dotted linework along the lower-left horizontal and vertical dotted boundary and along the dotted line inside/near the upper-left corridor. Per the reference, these dotted lines are treated as administrative/survey linework unless corroborated by physical symbols.

4. **Single solid enclosure / field boundaries**  
   Confidence: high.  
   Marked thin solid linework forming field, garden, and enclosure divisions in the central planted block, the right-hand fields, and the lower-right boundary. These are not marked as walkable paths.

5. **Deciduous tree symbols**  
   Confidence: high.  
   Marked repeated rounded/puffy tree symbols around the compound, along roads and boundaries, and in the lower-right field edge. Markers are representative repeats rather than a single region marker.

6. **Coniferous tree symbols**  
   Confidence: high.  
   Marked triangular pine-like symbols in the lower-left row and along the vertical/lower central boundary where the conifer form is clear.

7. **Planted / mixed enclosure**  
   Confidence: medium-high.  
   Lightly outlined and tinted the tight central enclosure containing internal divisions, planting texture, and repeated tree marks. The class is supported by the visible enclosure planting and tree-symbol mixture, but the exact vegetation type is not specified beyond the OS-symbol class.

8. **Rough vegetation / ditch / scrub strip**  
   Confidence: medium-low.  
   Lightly outlined the irregular clustered strip south of the buildings and added representative markers. This may overlap with yard-edge planting or ditch/scrub symbols, so it is marked as a cautious class rather than a definite terrain interpretation.

9. **Printed map text**  
   Confidence: high.  
   Marked the printed place-name text `Grove`. This is treated only as map lettering, not an in-world sign or structure.

10. **Open fields / ordinary ground**  
    Confidence: high.  
    Marked representative open stippled areas where no specific OS symbol is visible. The stippled base is not interpreted as crops, water, marsh, or any other feature by itself.

## Absence and uncertainty notes

- No confident paired dashed unfenced path/track symbol is visible in the crop, so no separate path/track marker class was placed.
- No water, marsh/bog, quarry, bridge, rock, well, spring, or slope/cliff symbol is visible in the crop.
- Tree markers are representative, not exhaustive; they are repeated enough to show the class distribution without obscuring the original crop.
- The rough vegetation/ditch strip and the planted enclosure are the only area treatments; both are kept tight to visible symbol clusters and do not imply features outside their outlines.
