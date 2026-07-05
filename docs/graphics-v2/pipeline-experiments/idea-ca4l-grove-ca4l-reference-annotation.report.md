# Grove CA4L Reference Annotation Report

Output plate: `docs/graphics-v2/pipeline-experiments/idea-ca4l-grove-ca4l-reference-annotation.png`

Basis: clean visual read of the supplied Grove map crop against the supplied 6-inch Ordnance Survey key/reference sheet. No prior annotation outputs were used. The reference sheet was not modified.

## Marker Classes

1. **Roofed structures**
   - Confidence: high for the dark or hatched rectangular footprints near the yard; moderate for the larger east-side rectangular form.
   - Notes: marked each likely visible roofed/built footprint individually. The larger outlined rectangle east of the central yard may be a roofed/built footprint or a small enclosure; it is therefore treated as less certain.

2. **Road / lane corridor**
   - Confidence: high.
   - Notes: paired solid margins mark broad physical lanes or road-like corridors, especially the upper-left diagonal corridor and the north/east diagonal approach. Dots are placed along the visible corridor geometry.

3. **Possible path / track**
   - Confidence: medium-low.
   - Notes: faint paired or broken traces south/east of the yard resemble the reference-sheet path/track examples, but the marks are weak and may partly be field or ditch linework.

4. **Dotted survey/admin boundary**
   - Confidence: high.
   - Notes: single bold dotted chains are marked separately from physical roads or hedges. These are not interpreted as walls unless corroborated by other linework or vegetation symbols.

5. **Road plus dotted boundary overlay**
   - Confidence: medium.
   - Notes: the upper-left corridor includes a dotted chain in or along the road corridor. The corridor itself is physical; the dotted component may be survey/admin information.

6. **Single solid enclosure / field boundaries**
   - Confidence: high.
   - Notes: thin single solid lines around enclosures, gardens, fields, and parcels are marked as boundaries rather than walkable paths.

7. **Deciduous tree symbols**
   - Confidence: high.
   - Notes: puffy circular tree symbols appear repeatedly along boundaries, roads, and within/near the planted enclosure. Multiple representative symbols are marked in place.

8. **Coniferous tree symbols**
   - Confidence: high.
   - Notes: tight triangular tree symbols are visible mostly along the western/southern tree line and near the south-central boundary. Representative repeated symbols are marked.

9. **Planted / mixed enclosure**
   - Confidence: high.
   - Notes: the regular enclosed planting area west/northwest of the yard contains internal texture and tree marks, matching planted or mixed enclosure examples in the reference sheet.

10. **Possible rough scrub / ditch block**
    - Confidence: medium-low.
    - Notes: irregular ground strokes around the lower yard/enclosure edge may indicate rough vegetation, ditch, or scrub texture. This overlaps visually with planted/enclosure texture, so the class is marked as possible rather than certain.

11. **Printed map text**
    - Confidence: high.
    - Notes: `Grove` is printed place-name text on the map. It should not be treated as an in-world sign or physical object.

12. **Open fields / ordinary ground**
    - Confidence: high.
    - Notes: unmarked stippled/background areas are marked only as representative open ground or ordinary field surface, not as a distinct constructed feature.

## Absence Notes

- No bridge, well/spring, contour, marsh, ford, or quarry/pit symbol is confidently visible in this crop.
- No enclosed feature was marked as a physical stone wall solely because of dotted survey/admin linework.
