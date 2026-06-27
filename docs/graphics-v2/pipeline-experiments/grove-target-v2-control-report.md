# Prototype Map Control Report

Input: `docs/graphics-v2/grove-map-target-site-crop.png`
Input size: 1200x820
Connected dark components: 229
Building-like components: 82
Small symbol-like components: 102

These counts are heuristic, not authoritative. The point of this pass is
to produce control images for clean-context image-generation experiments
without hand-authored per-location interpretation.

Outputs:
- `grove-target-v2-ink-mask.png`
- `grove-target-v2-semantic-mask.png`
- `grove-target-v2-oblique-raw-warp.png`
- `grove-target-v2-extruded-blockout.png`
