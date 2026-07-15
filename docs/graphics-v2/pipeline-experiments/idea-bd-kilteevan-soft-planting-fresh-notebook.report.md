# Audit Report

Generated image:
`/Users/dmooney/Rundale/docs/graphics-v2/pipeline-experiments/idea-bd-kilteevan-soft-planting-fresh-notebook.png`

Built-in source image:
`/Users/dmooney/.codex/generated_images/019f0f28-cff9-79b1-a015-0328d85330b0/ig_0c99c41b77d69328016a41526dfe8c819093728a35a8be56e4.png`

Generation mode: built-in `image_gen`, one generated image.

Dimensions: 1672 x 941 PNG, effectively 16:9.

## Visual Audit

- Camera/orientation: mostly successful low 3/4 orthographic/isomorphic plate with visible facades, roofs, thresholds, and no horizon. The scene remains broadly north-up rather than being rotated into a fully diagonal postcard composition.
- Broad road topology: mixed. Roads are broad, muddy, continuous, and walkable, entering/leaving frame edges. However, the rendered road network still regularizes into a scenic central junction more than the source crop probably wants.
- Extra paths: mixed/fail. The garden area contains several tan walkable-looking internal paths and a strong lower road/track around the planting block. Some may read as planted-bed access, but several are more path-like than the prompt requested.
- Soft planting vs walls: mixed/fail. The garden/orchard reads as vegetation and planted beds, but multiple perimeter and internal lines became hard fence/wall/trellis edges. The lower-right diagonal vegetation trace also appears to preserve a suppressed/no-data/admin-like line as a physical hedge/tree chain.
- Buildings/doors: mostly pass. Visible walkable buildings have readable timber plank doors fitted into openings, with thresholds or yard/road access. I did not see empty person-sized black door holes; small shed doors are dark but appear planked.
- Roof discipline: pass. I do not see chimneys, smoke, roof stacks, vents, or obvious isolated roof nubs.
- Semantic leakage: pass. No UI, labels, signs, copied map text, people, animals, carts, water, church, graveyard, shop, bridge, or smoke are visible.
- Overall: visually strong notebook-style plate with good door and roof discipline, but not a clean topology proof. The main weaknesses are boundary materialization and possible admin/no-data line leakage, plus a lingering scenic-junction bias.

## Close-Up Door Audit

Follow-up crop inspection checked the upper houses, the main foreground house,
and the two small foreground sheds. These visible person-sized openings read as
timber plank door faces fitted into the openings rather than empty black holes.
This is the main improvement from the "DOORS ON OPENINGS" prompt wording.
