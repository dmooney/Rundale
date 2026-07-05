# CA Grove OS-Key Annotated Map Prompt

Built-in imagegen was given two visible reference images:

- `docs/graphics-v2/web-references/os-6inch-map-key/os-6inch-map-key-reference-sheet.png`
  as a symbol-interpretation reference.
- `docs/graphics-v2/grove-map-target-site-crop.png` as the annotation target.

Prompt:

```text
Create an annotated analysis plate for the Grove historic 6-inch Ordnance
Survey source crop using the OS 6-inch map key reference sheet as
symbol-interpretation guidance. Preserve the Grove source crop as the central
background image. Add transparent colored overlays, arrows, and readable labels
around the crop margins. Include a compact legend explaining the overlay
colors. The result should look like careful cartographic analysis markup, not
game art and not a scenic render.

Annotate structures, physical boundaries, administrative/survey boundaries,
roads and lanes, paths/yards/access, ground features such as fields and planted
enclosures, rivers/water/bogs if present, trees and planting, crop/planting
rows, and printed map text.

Mark uncertainty. Do not invent features not visible in the Grove crop. Do not
convert dotted/pecked administrative/survey lines into paths or walls. Mark the
printed word Grove as map label/text, not an in-world sign. Use short readable
labels and a colored legend.
```

Post-generation correction:

- The generated subtitle incorrectly said the source crop was from the Scoilnet
  factsheet. That subtitle was locally corrected to identify the Grove crop as
  the source and Scoilnet as the OS key reference.
