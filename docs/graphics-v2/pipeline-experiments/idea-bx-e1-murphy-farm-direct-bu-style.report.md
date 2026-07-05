# Idea BX E1 Murphy's Farm Direct BU-Style Report

Purpose: direct Murphy's Farm render using the established Graphics V2
map/control/camera/reference stack.

Input stack:

- `idea-bx-murphy-farm-z17-map-crop.png` — north-up source crop from the
  Roscommon 1st edition z17 tile source.
- `idea-bx-murphy-farm-control-soft-planting-control.png` — deterministic
  soft-planting/material control, used as an aid rather than building truth.
- `idea-bx-murphy-farm-control-oblique-raw-warp.png` — deterministic oblique
  camera cue.
- `idea-bu-e2-bu-e1-concept-realism-final-tighten.png` — BU E2 concept-realism
  style target.
- Door-fixed slate/thatch crops and the Irish dry-stone wall reference sheet.

Result: `idea-bx-e1-murphy-farm-direct-bu-style.png`.

Audit:

- Pass: west/left textured terrain is interpreted as dark peat bog / bog-edge
  ground rather than ordinary field texture.
- Pass: main farm doors are readable plank doors with thresholds; no obvious
  black doorway void dominates the walkable facades.
- Pass: the plate has the BU E2 warm, worn, hand-inked watercolor finish and
  neutral daylight suitable for runtime overlays.
- Caveat: a small square chimney / roof-nub artifact appears on the main
  thatched roof.
- Caveat: boundaries still lean more stone-wall-like than the Roscommon
  hedge/bank/ditch default.

Disposition: useful direct recipe evidence, but not the preferred Murphy output.
Cycle BX E2 applies one bounded correction for roof and boundary material while
preserving this layout.
