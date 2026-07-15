# CA3 Grove OS-Key Numbered Annotation Refined Prompt

Built-in imagegen was given:

- `docs/graphics-v2/grove-map-target-site-crop.png` as the annotation target.
- `docs/graphics-v2/web-references/os-6inch-map-key/os-6inch-map-key-reference-sheet.png`
  as symbol-interpretation guidance.
- `docs/graphics-v2/pipeline-experiments/idea-ca2-grove-os-key-numbered-annotation.png`
  as a format reference.

Refinement requested from CA2:

- Use numbered markers, no arrows or leader lines.
- Mark puffy-circle tree icons as deciduous trees, and mark many of them
  throughout the crop.
- Keep triangular/conifer-like icons distinct from deciduous trees.
- Interpret a bold dotted line between two parallel road-edge lines as a road
  with an administrative boundary in the middle: the road is physical, the
  dotted centerline is non-physical.
- Interpret parallel light dashed lines heading west from the main structures
  as an unfenced path, not a wall or administrative boundary.
- Interpret the rough block south of the buildings, with trees and unclear
  marks, as likely overgrown ditch / scrub / hedgerow.
- Treat other solid single lines as field/enclosure boundaries, probably
  hedges, banks, ditches, fences, or short rough dry-stone walls in Roscommon;
  do not promote them to paths.

Post-generation correction:

- The generated subtitle incorrectly described the Grove source crop as coming
  from the Scoilnet factsheet. That line was locally patched to identify the
  Grove crop as the source and Scoilnet as the OS key reference.
