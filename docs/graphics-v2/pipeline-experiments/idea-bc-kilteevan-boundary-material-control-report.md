# Prototype Map Control Report

Input: `docs/graphics-v2/pipeline-experiments/idea-ar-kilteevan-playable-no-admin-map-crop.png`
Original comparison: `docs/graphics-v2/pipeline-experiments/idea-ar-kilteevan-playable-map-crop.png`
Input size: 512x288
Connected dark components: 115
Building-like components: 0
Small symbol-like components: 85
Suppressed/no-data comparison pixels: 5344
Soft planting/material pixels: 36874

These counts are heuristic, not authoritative. The point of this pass is
to produce control images for clean-context image-generation experiments
without hand-authored per-location interpretation.

Outputs:
- `idea-bc-kilteevan-boundary-material-ink-mask.png`
- `idea-bc-kilteevan-boundary-material-semantic-mask.png`
- `idea-bc-kilteevan-boundary-material-literal-paint-control.png`
- `idea-bc-kilteevan-boundary-material-literal-paint-oblique.png`
- `idea-bc-kilteevan-boundary-material-boundary-material-control.png`
- `idea-bc-kilteevan-boundary-material-boundary-material-oblique.png`
- `idea-bc-kilteevan-boundary-material-oblique-raw-warp.png`
- `idea-bc-kilteevan-boundary-material-oblique-ink-warp.png`
- `idea-bc-kilteevan-boundary-material-linework-control.png`
- `idea-bc-kilteevan-boundary-material-road-topology-control.png`
- `idea-bc-kilteevan-boundary-material-road-topology-oblique.png`
- `idea-bc-kilteevan-boundary-material-extruded-blockout.png`
