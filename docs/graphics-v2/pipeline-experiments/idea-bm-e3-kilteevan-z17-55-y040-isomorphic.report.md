# E3: Kilteevan native z17 source / y_squash 0.40

Generated with built-in `image_gen` from the prompt sidecar.

- Output: `idea-bm-e3-kilteevan-z17-55-y040-isomorphic.png`
- Prompt: `idea-bm-e3-kilteevan-z17-55-y040-isomorphic.prompt.md`
- Generated cache source: `/Users/dmooney/.codex/generated_images/019f0fee-e45e-7890-85a7-ed0dc4099c99/ig_0842d16fe553182e016a419de9f35c8193b1ebfc60f5c3b0a8.png`
- Dimensions: `1672x941`

## Verdict

Native z17 source authority retained the lower-camera scale but did not by itself solve wall/path semantics. It remains useful evidence that source resolution matters, while also showing prompt/control semantics still dominate boundary materialization.

## Acceptance Read

- Camera/zoom: PASS for the BM goal of lowering the camera and enlarging facades/doors.
- Cartographic topology: PARTIAL. Major roads/building groups remain recognizable, but garden/boundary semantics are still the fragile part.
- Doors/roofs: PASS by visual inspection at full-frame scale; no obvious chimneys/smoke/roof nubs seen in the accepted BM outputs.
- Hard semantic leaks: PASS by visual inspection; no UI/text/people/animals/water/church/shop leakage seen.

## Follow-up Signal

Use `55%` closer crops plus `y_squash=0.40` as the next transform baseline. Do not spend more imagegen calls on camera wording alone; the next accuracy gain should come from a better upstream path/wall/garden material control.
