# Cycle AV2 Audit

Generated with the built-in `image_gen` tool from the exact prompt in `idea-av-kilteevan-symbolic-two-step-isomorphic.prompt.md`.

## Result

Selected output: `idea-av-kilteevan-symbolic-two-step-isomorphic.png`

## Success Criteria Audit

- 16:9 no-UI plate: pass. No visible UI, labels, survey text, map pins, people, animals, carts, water, church, shop, smoke, or weather effects observed.
- Notebook style: pass. The render uses sepia ink, watercolor vegetation, muddy rutted roads, mottled fields, slate/thatch textures, and readable rural facades.
- Low 3/4 isomorphic camera: partial pass. The plate has visible roofs, facades, doors, thresholds, and stable walkable ground, though it still reads somewhat high/survey-like in the open field and garden zones.
- Roads/yards walkable: pass. Broad lanes remain continuous and mostly clear, with buildings and vegetation kept out of the road centers.
- Building treatment: partial pass. Major visible buildings have readable dark doorways/thresholds and no obvious chimneys; however, the output appears to add/regularize several small sheds beyond what the tight map crop may support.
- Open-field/admin-boundary discipline: partial fail. Open fields remain broadly readable, but several continuous wall/hedge chains and garden/compound outlines persist, especially around the central planting area and road edges, suggesting leakage from the symbolic control rather than strict map-veto behavior.
- Crop/composition lock: partial fail. The result is attractive and usable, but it trends toward a centered scenic crossroads composition rather than preserving the awkward tight crop behavior.
- Roof rule: pass on visual inspection. No clear chimneys, smoke, roof stacks, or obvious roof nubs observed.

## Overall

Usable as a visually strong AV2 candidate, but not a clean recipe success. Main issues are over-materialized continuous boundaries, likely symbolic-control leakage, and composition regularization toward a scenic crossroads.
