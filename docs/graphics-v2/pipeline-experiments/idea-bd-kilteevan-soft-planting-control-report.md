# Prototype Map Control Report

Input: `docs/graphics-v2/pipeline-experiments/idea-ar-kilteevan-playable-no-admin-map-crop.png`
Original comparison: `docs/graphics-v2/pipeline-experiments/idea-ar-kilteevan-playable-map-crop.png`
Input size: 512x288
Connected dark components: 115
Building-like components: 0
Small symbol-like components: 85
Suppressed/no-data comparison pixels: 5344
Soft planting/material pixels: 36874
Soft planting suppressed-control pixels: 39341
Soft planting suppressed-control core pixels: 38870
Soft planting suppressed-control edge pixels: 471

These counts are heuristic, not authoritative. The point of this pass is
to produce control images for clean-context image-generation experiments
without hand-authored per-location interpretation.

Outputs:

- `idea-bd-kilteevan-soft-planting-ink-mask.png`
- `idea-bd-kilteevan-soft-planting-semantic-mask.png`
- `idea-bd-kilteevan-soft-planting-literal-paint-control.png`
- `idea-bd-kilteevan-soft-planting-literal-paint-oblique.png`
- `idea-bd-kilteevan-soft-planting-boundary-material-control.png`
- `idea-bd-kilteevan-soft-planting-boundary-material-oblique.png`
- `idea-bd-kilteevan-soft-planting-soft-planting-control.png`
- `idea-bd-kilteevan-soft-planting-soft-planting-oblique.png`
- `idea-bd-kilteevan-soft-planting-oblique-raw-warp.png`
- `idea-bd-kilteevan-soft-planting-oblique-ink-warp.png`
- `idea-bd-kilteevan-soft-planting-linework-control.png`
- `idea-bd-kilteevan-soft-planting-road-topology-control.png`
- `idea-bd-kilteevan-soft-planting-road-topology-oblique.png`
- `idea-bd-kilteevan-soft-planting-extruded-blockout.png`
