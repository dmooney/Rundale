# Direct-From-Control - Cycle AA

Cycle AA tests whether the current style/topology target can be reached directly
from:

1. a tight local topology control crop,
2. a deterministic oblique pitch cue,
3. the original historic map crop,
4. the original illustrated parish notebook sample, and
5. cleaned doorway/material style crops.

Unlike X/Y/Z, AA does not pass a previous rendered plate as a structure or style
target. This is closer to the desired production pipeline for many locations:
local map/control input plus a reusable prompt/reference stack.

## Outputs

| Site         | Output                                                                 | Prompt                                                                       | Report                                                                       | Result                                       |
| ------------ | ---------------------------------------------------------------------- | ---------------------------------------------------------------------------- | ---------------------------------------------------------------------------- | -------------------------------------------- |
| Beechwood AA | `pipeline-experiments/idea-aa-beechwood-direct-control-low-camera.png` | `pipeline-experiments/idea-aa-beechwood-direct-control-low-camera.prompt.md` | `pipeline-experiments/idea-aa-beechwood-direct-control-low-camera.report.md` | Direct-control pass, style/camera regression |
| Grove AA     | `pipeline-experiments/idea-aa-grove-direct-control-low-camera.png`     | `pipeline-experiments/idea-aa-grove-direct-control-low-camera.prompt.md`     | `pipeline-experiments/idea-aa-grove-direct-control-low-camera.report.md`     | Direct-control pass, style/camera regression |

## Audit Questions

- Does AA preserve the same topology as the local control crop without a prior
  rendered plate?
- Does AA reach the notebook-style camera/texture quality of the Z pair?
- Does AA avoid semantic leakage from the full notebook sample?
- Are all visible playable building facades given readable doors/thresholds?
- If AA fails, is the failure topology, style, camera, or missing repair pass?

## Result

AA is useful evidence, but not the new visual endpoint.

Both AA plates demonstrate that the local control crop can drive a coherent
direct render without passing in X/Y/Z as previous rendered plates. Beechwood AA
keeps a connected compound, and Grove AA keeps separated yard buildings, so the
topology signal survives the clean-context direct-from-control setup.

The regression is style and camera. Compared with the Z pair and the original
illustrated parish notebook sample, both AA plates are cleaner, higher, and
more survey-board-like. Garden rows, stone walls, roof grids, and road edges
regularize too much; roofs dominate more than facades; the images feel like
controlled isometric map plates rather than rough ink-and-watercolor notebook
scenes.

The next cycle should preserve AA's direct-control purity but replace the
leaky slate style crop with the cleaned single-building slate crop, keep the
single-building thatch crop, and push harder on close playable crop scale,
low-camera facades, irregular watercolor, and anti-regularity language.
