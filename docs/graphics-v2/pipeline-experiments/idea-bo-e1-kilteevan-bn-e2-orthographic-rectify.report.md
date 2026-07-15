# BO E1: BN E2 orthographic rectification

Generated with built-in `image_gen` from the prompt sidecar.

- Output: `idea-bo-e1-kilteevan-bn-e2-orthographic-rectify.png`
- Prompt: `idea-bo-kilteevan-bn-e2-orthographic-rectify.prompt.md`
- Generated cache source: `/Users/dmooney/.codex/generated_images/019f0fee-e45e-7890-85a7-ed0dc4099c99/ig_08fc292c3b17d9a2016a425c9d4acc8196be848d0b86d6dc39.png`
- Dimensions: `1672x941`
- Comparison plate: `../cartographic-comparisons/bo-orthographic-rectification-comparison.png`

## Verdict

Useful geometry proof, but not the preferred candidate. BO E1 reduces BN E2's
fisheye/barrel feel and makes the roads, buildings, and garden block read more
like a parallel-projection game plate.

The cost is semantic/style overbuild. The render becomes cleaner and more
diagrammatic, adding post-and-rail fence language and hardening boundary/garden
edges. That is exactly the failure mode the pipeline has been trying to avoid.

## Acceptance Read

- Orthographic rectification: PASS. The plate is straighter and less lensy than
  BN E2.
- Camera/zoom: PASS/PARTIAL. Low camera survives, but the cleaner projection
  slightly weakens the rough concept-art feel.
- Topology/content preservation: PARTIAL. Main scene structure survives, but
  added fence/boundary language changes the material read.
- Doors/roofs: PASS at full-frame scale; visible enterable facades retain plank
  doors and no obvious chimneys/smoke.
- Recipe status: FAIL. Do not use as final target because it overbuilds walls
  and fences.

## Follow-Up Signal

Rectification should be a softer "barrel correction only" pass, not a general
orthographic redraw.
