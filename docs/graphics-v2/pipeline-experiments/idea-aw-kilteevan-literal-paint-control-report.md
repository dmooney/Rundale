# Prototype Map Control Report

Input: `docs/graphics-v2/pipeline-experiments/idea-ar-kilteevan-playable-no-admin-map-crop.png`
Original comparison: `docs/graphics-v2/pipeline-experiments/idea-ar-kilteevan-playable-map-crop.png`
Input size: 512x288
Connected dark components: 115
Building-like components: 0
Small symbol-like components: 85
Suppressed/no-data comparison pixels: 5344

These counts are heuristic, not authoritative. The point of this pass is
to produce control images for clean-context image-generation experiments
without hand-authored per-location interpretation.

Outputs:
- `idea-aw-kilteevan-literal-paint-ink-mask.png`
- `idea-aw-kilteevan-literal-paint-semantic-mask.png`
- `idea-aw-kilteevan-literal-paint-literal-paint-control.png`
- `idea-aw-kilteevan-literal-paint-literal-paint-oblique.png`
- `idea-aw-kilteevan-literal-paint-oblique-raw-warp.png`
- `idea-aw-kilteevan-literal-paint-oblique-ink-warp.png`
- `idea-aw-kilteevan-literal-paint-linework-control.png`
- `idea-aw-kilteevan-literal-paint-road-topology-control.png`
- `idea-aw-kilteevan-literal-paint-road-topology-oblique.png`
- `idea-aw-kilteevan-literal-paint-extruded-blockout.png`
