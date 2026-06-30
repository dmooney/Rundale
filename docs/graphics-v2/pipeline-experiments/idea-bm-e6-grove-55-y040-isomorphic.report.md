# E6: Grove 55% / y_squash 0.40

Generated with built-in `image_gen` from the prompt sidecar.

- Output: `idea-bm-e6-grove-55-y040-isomorphic.png`
- Prompt: `idea-bm-e6-grove-55-y040-isomorphic.prompt.md`
- Generated cache source: `/Users/dmooney/.codex/generated_images/019f0fee-e45e-7890-85a7-ed0dc4099c99/ig_00b271a851c2dbff016a419f8093e8819481e2750c4f4f4273.png`
- Dimensions: `1672x941`

## Verdict

Good lower-camera generalization to a separate-building topology. Facades and doors are clearer than BH. It simplifies some Grove nuance and keeps the garden fairly rectilinear, but does not collapse the separate-building yard into one compound.

## Acceptance Read

- Camera/zoom: PASS for the BM goal of lowering the camera and enlarging facades/doors.
- Cartographic topology: PARTIAL. Major roads/building groups remain recognizable, but garden/boundary semantics are still the fragile part.
- Doors/roofs: PASS by visual inspection at full-frame scale; no obvious chimneys/smoke/roof nubs seen in the accepted BM outputs.
- Hard semantic leaks: PASS by visual inspection; no UI/text/people/animals/water/church/shop leakage seen.

## Follow-up Signal

Use `55%` closer crops plus `y_squash=0.40` as the next transform baseline. Do not spend more imagegen calls on camera wording alone; the next accuracy gain should come from a better upstream path/wall/garden material control.
