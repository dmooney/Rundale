# E5: Beechwood 55% / y_squash 0.40

Generated with built-in `image_gen` from the prompt sidecar.

- Output: `idea-bm-e5-beechwood-55-y040-isomorphic.png`
- Prompt: `idea-bm-e5-beechwood-55-y040-isomorphic.prompt.md`
- Generated cache source: `/Users/dmooney/.codex/generated_images/019f0fee-e45e-7890-85a7-ed0dc4099c99/ig_0c2f7bbc4c3e797c016a419f1898648190aa9d744eb3d3f89f.png`
- Dimensions: `1672x941`

## Verdict

Strong generalization. It is lower and closer than BJ, with substantially clearer facades/doors, and it preserves the connected-compound read well. The garden edge is still structured, but the camera/zoom target is much closer.

## Acceptance Read

- Camera/zoom: PASS for the BM goal of lowering the camera and enlarging facades/doors.
- Cartographic topology: PARTIAL. Major roads/building groups remain recognizable, but garden/boundary semantics are still the fragile part.
- Doors/roofs: PASS by visual inspection at full-frame scale; no obvious chimneys/smoke/roof nubs seen in the accepted BM outputs.
- Hard semantic leaks: PASS by visual inspection; no UI/text/people/animals/water/church/shop leakage seen.

## Follow-up Signal

Use `55%` closer crops plus `y_squash=0.40` as the next transform baseline. Do not spend more imagegen calls on camera wording alone; the next accuracy gain should come from a better upstream path/wall/garden material control.
