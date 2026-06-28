# Report

Mode: built-in `image_gen` edit, with Image 1 as the topology authority and Images 2-5 as style/material references.

Saved output: `docs/graphics-v2/pipeline-experiments/idea-ay-kilteevan-au-notebook-style-refine.png`

Audit summary:
- Topology preservation: broadly preserved. The cottage cluster, main house and sheds, roads, garden block, gates/fences, tree masses, and open-field relationships remain recognizable from Image 1. This is not pixel-perfect: the usual image-model micro-warping is present, but it does not appear recomposed into a new crossroads scene. The source and output are both `1672x941`.
- Style refinement: successful. The result has denser sepia ink, rougher watercolor vegetation, more mottled fields, dirtier muddy road texture, and clearer limewashed/slate building surfaces.
- Door rule: appears satisfied on visual audit. Every person-sized visible building opening I could identify in the top cottage cluster, main house, and foreground sheds has a visible brown or gray-brown plank door and a readable threshold/step. Small square openings remain window-like.
- Boundary/chimney avoid list: no UI, text, people, animals, church, river, bridge, shopfront, smoke, or obvious chimneys/roof nubs are visible. Existing garden fences were roughened but I do not see new continuous stone-wall networks added around the open fields.
