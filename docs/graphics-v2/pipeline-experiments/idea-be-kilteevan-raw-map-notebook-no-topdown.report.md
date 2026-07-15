# Audit Report

Generated with the built-in `image_gen` tool as one fresh image from the seven supplied inputs and the saved prompt.

Image path: `docs/graphics-v2/pipeline-experiments/idea-be-kilteevan-raw-map-notebook-no-topdown.png`
Generated source: `/Users/dmooney/.codex/generated_images/019f0f38-be13-74b1-ab71-e72c5f186429/ig_0b396ce99d5087c4016a41566d76708195823426c433ec3883.png`
Dimensions: 1672 x 941 PNG, wide 16:9-ish desktop plate.

## Candid Visual Audit

- Camera/style: Mostly successful low 3/4 orthographic notebook feel. Roofs, facades, doors, thresholds, muddy roads, ink hatching, watercolor fields, and rough vegetation are readable. The camera still reads a little higher and more survey-like than the target 30-35 degree playable camera.
- North-up / map orientation: Not mechanically provable from the render alone. The plate keeps a stable oblique ground plane, but the local plan has been regularized into a scenic central road composition rather than staying awkwardly map-crop faithful.
- Broad road topology and exits: Partial/fail. Roads are broad and continuous with edge exits, but the render forms a composed crossroads/Y-like junction and smooths/curves the road plan. This likely adds or reshapes road connectivity beyond Images 1-2.
- Absence of extra paths: Partial/fail. There are extra worn yard/road-like connectors through the center and around buildings; some may be yard wear, but they read as added paths/branches.
- Garden/internal marks as planting, not walls: Partial/fail. The garden area has planted beds and texture, but stone-wall enclosure treatment is too strong and continuous, especially around the central planted compound.
- Admin/survey boundary suppression: Mixed. I do not see a direct copied dotted/pecked seam, but the image over-materializes boundaries as stone walls in ways that risk reading like suppressed linework became terrain.
- Open fields: Partial pass. Some fields remain soft and open, especially upper/right areas, but continuous walls and enclosure edges reduce the open-field-first read.
- Buildings: Mixed. The image has a plausible rural vernacular set and keeps buildings off road centers, but it invents a more balanced cottage grouping than the map crop appears to support.
- Doors on openings: Pass on visible walkable openings from this inspection. Every person-sized visible entrance appears to have a timber plank door fitted into it, with thresholds or yard access. Dark windows remain window-sized rather than door holes.
- Chimneys / nubs / smoke: Mostly pass but not perfect. No smoke and no obvious chimney stacks; one small roof-edge mark on the upper-center building could be read as a tiny roof nub at close inspection.
- Semantic leaks: Mostly clean for text/UI/people/animals/water/church/shop/signs. However, the right-hand cottage includes barrel/tub-like props, which violates the prompt's no-barrels constraint.

Overall: strong notebook style and door discipline, but not a topology-success plate. The main failures are scenic crossroads regularization, too much continuous stone walling, extra path/yard connectors, invented balanced building composition, and a small prop leak.
