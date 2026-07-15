# BN E2: Kilteevan north-extended 10-degree / 50%-lower increment

Generated with built-in `image_gen` from the prompt sidecar.

- Output: `idea-bn-e2-kilteevan-north-10deg-incremental.png`
- Prompt: `idea-bn-e2-kilteevan-north-10deg-incremental.prompt.md`
- Generated cache source: `/Users/dmooney/.codex/generated_images/019f0fee-e45e-7890-85a7-ed0dc4099c99/ig_085960df4abe9e1c016a41d5544f3881909c5f28708d8c5b37.png`
- Dimensions: `1672x941`
- Comparison plate: `../cartographic-comparisons/bn-e2-10deg-incremental-comparison.png`

## Verdict

Best BN camera result. E2 is the first output in this branch that visibly
answers the "50% lower" request: the main facades, plank doors, wall bases, and
foreground sheds become much more dominant, while roofs remain visible as
shallow tops. It no longer feels primarily like a survey-board render.

The far/top/north background also stays anchored to the extended map evidence.
That confirms the central BN hypothesis: a very low orthographic camera needs
source coverage far beyond the playable core.

The caveat is semantic. At this lower angle, fences, garden edges, and boundary
marks become more emphatic. E2 is a camera proof, not a final topology recipe.

## Acceptance Read

- Camera/zoom: PASS for the BN target. This is materially lower than BM E4 and
  BN E1.
- Cartographic topology: PARTIAL. The main building cluster, upper group,
  right-side road, garden/orchard region, and broad road layout remain
  recognizable, but exact path/wall/garden semantics remain fragile.
- North/background sourcing: PASS. The top/background reads as compressed
  source-map context rather than invented sky, hills, church, water, or filler.
- Doors/roofs: PASS at full-frame scale; every obvious visible facade has a
  fitted plank door/threshold and no obvious chimneys or smoke are visible.
- Feature semantics: PARTIAL/FAIL for recipe status. Garden rows and boundaries
  still harden into wall/fence material.

## Follow-Up Signal

Keep the 10-degree north-extended source/cue idea as the camera target, but do
not continue prompt-only camera retries. The next improvement needs stronger
material/semantic controls so roads, paths, gardens, hedges, ditches, walls,
and admin/no-data lines are differentiated before imagegen.
