# BO E2: BN E2 soft orthographic rectification

Generated with built-in `image_gen` from the prompt sidecar.

- Output: `idea-bo-e2-kilteevan-bn-e2-soft-orthographic-rectify.png`
- Prompt: `idea-bo-e2-kilteevan-bn-e2-soft-orthographic-rectify.prompt.md`
- Generated cache source: `/Users/dmooney/.codex/generated_images/019f0fee-e45e-7890-85a7-ed0dc4099c99/ig_00319f170c612a7d016a425dbf1b748197817fbbdd2b520e9c.png`
- Dimensions: `1672x941`
- Comparison plate: `../cartographic-comparisons/bo-orthographic-rectification-comparison.png`

## Verdict

Best BO candidate. E2 preserves the main BN E2 content, low camera, source-backed
northern background, door discipline, and rough notebook style while reducing
the most obvious fisheye/barrel distortion. It does not copy BO E1's heavy
post-and-rail fence network.

The remaining weakness is still garden/wall material semantics. The garden
block is straighter and more usable as a game plate, but it remains hard-edged
and fairly diagrammatic.

## Acceptance Read

- Orthographic rectification: PASS. The plate is less bowed/lensy than BN E2
  and more usable as a parallel-projection background.
- Camera/zoom: PASS. The low BN E2 facade/door scale survives better than in
  earlier high-isometric candidates.
- Topology/content preservation: PASS/PARTIAL. Main buildings, roads, garden,
  tree masses, right-side road, and north/background content remain broadly
  recognizable. The exact garden material read is still not solved.
- Doors/roofs: PASS at full-frame scale; no obvious dark-void doors, chimneys,
  roof nubs, smoke, people, UI, water, or church leakage seen.
- Recipe status: PARTIAL. Use as the best current final-step rectification
  candidate, but keep boundary/material semantics as the next work item.

## Follow-Up Signal

The pipeline should split camera and projection:

```text
low-camera source-backed draft -> soft orthographic/barrel-correction pass
```

The rectification prompt must stay conservative: preserve soft ambiguity, do
not repaint, and explicitly forbid extra fences/walls.
