# Prototype Map Control Report

Input: `docs/graphics-v2/grove-map-target-site-crop.png`
Input size: 1200x820
Connected dark components: 229
Building-like components: 58
Small symbol-like components: 123

These counts are heuristic, not authoritative. The point of this pass is
to produce control images for clean-context image-generation experiments
without hand-authored per-location interpretation.

Outputs:
- `grove-target-v4-ink-mask.png`
- `grove-target-v4-semantic-mask.png`
- `grove-target-v4-oblique-raw-warp.png`
- `grove-target-v4-oblique-ink-warp.png`
- `grove-target-v4-linework-control.png`
- `grove-target-v4-extruded-blockout.png`
