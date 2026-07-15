# E4: Kilteevan path/wall semantics / y_squash 0.40

Generated with built-in `image_gen` from the prompt sidecar.

- Output: `idea-bm-e4-kilteevan-z17-55-y040-path-wall-isomorphic.png`
- Prompt: `idea-bm-e4-kilteevan-z17-55-y040-path-wall-isomorphic.prompt.md`
- Generated cache source: `/Users/dmooney/.codex/generated_images/019f0fee-e45e-7890-85a7-ed0dc4099c99/ig_0209820b3cdd94b6016a419e6eb52c8194bc33fa6bc0fa9bf6.png`
- Dimensions: `1672x941`

## Verdict

Best Kilteevan BM candidate. It keeps the E2/E3 lower-camera win and softens the wall/fence web relative to E2/E3. Garden/internal rows are still somewhat regular, so the reusable pipeline likely needs an upstream material/semantic control rather than more prose alone.

## Acceptance Read

- Camera/zoom: PASS for the BM goal of lowering the camera and enlarging facades/doors.
- Cartographic topology: PARTIAL. Major roads/building groups remain recognizable, but garden/boundary semantics are still the fragile part.
- Doors/roofs: PASS by visual inspection at full-frame scale; no obvious chimneys/smoke/roof nubs seen in the accepted BM outputs.
- Hard semantic leaks: PASS by visual inspection; no UI/text/people/animals/water/church/shop leakage seen.

## Follow-up Signal

Use `55%` closer crops plus `y_squash=0.40` as the next transform baseline. Do not spend more imagegen calls on camera wording alone; the next accuracy gain should come from a better upstream path/wall/garden material control.
