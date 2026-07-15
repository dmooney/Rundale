# E1: Kilteevan 70% / y_squash 0.48

Generated with built-in `image_gen` from the prompt sidecar.

- Output: `idea-bm-e1-kilteevan-70-y048-isomorphic.png`
- Prompt: `idea-bm-e1-kilteevan-70-y048-isomorphic.prompt.md`
- Generated cache source: `/Users/dmooney/.codex/generated_images/019f0fee-e45e-7890-85a7-ed0dc4099c99/ig_04a95400b0468b10016a419c93ab0c819389449df73fa4fe6e.png`
- Dimensions: `1672x941`

## Verdict

Conservative lower-camera attempt. The camera is a little lower than BA and doors/facades are more readable, but it still feels somewhat high and estate-board-like. Garden and boundary lines remain too crisp/physical.

## Acceptance Read

- Camera/zoom: PARTIAL for the BM goal of lowering the camera and enlarging facades/doors.
- Cartographic topology: PARTIAL. Major roads/building groups remain recognizable, but garden/boundary semantics are still the fragile part.
- Doors/roofs: PASS by visual inspection at full-frame scale; no obvious chimneys/smoke/roof nubs seen in the accepted BM outputs.
- Hard semantic leaks: PASS by visual inspection; no UI/text/people/animals/water/church/shop leakage seen.

## Follow-up Signal

Use `55%` closer crops plus `y_squash=0.40` as the next transform baseline. Do not spend more imagegen calls on camera wording alone; the next accuracy gain should come from a better upstream path/wall/garden material control.
