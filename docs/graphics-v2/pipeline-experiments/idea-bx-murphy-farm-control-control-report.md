# Prototype Map Control Report

Input: `docs/graphics-v2/pipeline-experiments/idea-bx-murphy-farm-z17-map-crop.png`
Original comparison: none
Input size: 768x432
Connected dark components: 33
Building-like components: 0
Small symbol-like components: 13
Suppressed/no-data comparison pixels: 0
Soft planting/material pixels: 7107
Soft planting suppressed-control pixels: 7338
Soft planting suppressed-control core pixels: 7205
Soft planting suppressed-control edge pixels: 133

These counts are heuristic, not authoritative. The point of this pass is
to produce control images for clean-context image-generation experiments
without hand-authored per-location interpretation.

Outputs:

- `idea-bx-murphy-farm-control-ink-mask.png`
- `idea-bx-murphy-farm-control-semantic-mask.png`
- `idea-bx-murphy-farm-control-literal-paint-control.png`
- `idea-bx-murphy-farm-control-literal-paint-oblique.png`
- `idea-bx-murphy-farm-control-boundary-material-control.png`
- `idea-bx-murphy-farm-control-boundary-material-oblique.png`
- `idea-bx-murphy-farm-control-soft-planting-control.png`
- `idea-bx-murphy-farm-control-soft-planting-oblique.png`
- `idea-bx-murphy-farm-control-oblique-raw-warp.png`
- `idea-bx-murphy-farm-control-oblique-ink-warp.png`
- `idea-bx-murphy-farm-control-linework-control.png`
- `idea-bx-murphy-farm-control-road-topology-control.png`
- `idea-bx-murphy-farm-control-road-topology-oblique.png`
- `idea-bx-murphy-farm-control-extruded-blockout.png`
