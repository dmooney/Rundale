# Prototype Map Control Report

Input: `docs/graphics-v2/grove-map-target-site-crop.png`
Original comparison: none
Input size: 1200x820
Connected dark components: 229
Building-like components: 58
Small symbol-like components: 123
Suppressed/no-data comparison pixels: 0
Soft planting/material pixels: 97202
Soft planting suppressed-control pixels: 99789
Soft planting suppressed-control core pixels: 97268
Soft planting suppressed-control edge pixels: 2521

These counts are heuristic, not authoritative. The point of this pass is
to produce control images for clean-context image-generation experiments
without hand-authored per-location interpretation.

Outputs:

- `idea-bz-grove-subagent-control-ink-mask.png`
- `idea-bz-grove-subagent-control-semantic-mask.png`
- `idea-bz-grove-subagent-control-literal-paint-control.png`
- `idea-bz-grove-subagent-control-literal-paint-oblique.png`
- `idea-bz-grove-subagent-control-boundary-material-control.png`
- `idea-bz-grove-subagent-control-boundary-material-oblique.png`
- `idea-bz-grove-subagent-control-soft-planting-control.png`
- `idea-bz-grove-subagent-control-soft-planting-oblique.png`
- `idea-bz-grove-subagent-control-oblique-raw-warp.png`
- `idea-bz-grove-subagent-control-oblique-ink-warp.png`
- `idea-bz-grove-subagent-control-linework-control.png`
- `idea-bz-grove-subagent-control-road-topology-control.png`
- `idea-bz-grove-subagent-control-road-topology-oblique.png`
- `idea-bz-grove-subagent-control-extruded-blockout.png`
