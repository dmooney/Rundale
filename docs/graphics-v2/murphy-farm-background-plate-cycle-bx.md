# Murphy's Farm Background Plate — Cycle BX

Cycle BX applies the established BU-style reproducible pipeline to Murphy's
Farm, a fictional Rundale exterior location pinned near lat `53.63579941155877`,
lon `-8.079662971357214`.

## Inputs

- Source crop: `map-sources/murphy-farm-z17-map-crop.png`.
- Deterministic controls:
  - `pipeline-experiments/idea-bx-murphy-farm-control-soft-planting-control.png`
  - `pipeline-experiments/idea-bx-murphy-farm-control-oblique-raw-warp.png`
- Style target: `authorities/beechwood-concept-realism-bu-e2.png`.
- Door references:
  - `style-crops/illustrated-style-low-camera-slate-single-house-door-fixed.png`
  - `style-crops/illustrated-style-low-camera-thatched-single-house-door-fixed.png`
- Boundary material reference:
  `web-references/irish-dry-stone-walls/irish-dry-stone-wall-reference-sheet.png`.

## Peat Bog Interpretation

The source crop has a distinct textured field area to the west/left of the
farmstead mark. The user identified this as likely peat bog, so Cycle BX treats
it as peat bog / bog-edge terrain: dark wet turf, rough heather/grass, and only
subtle drainage or turf-bank hints. It is not treated as generic field, open
water, a new wall network, or extra road/path evidence.

## Outputs

- Direct render:
  `pipeline-experiments/idea-bx-e1-murphy-farm-direct-bu-style.png`.
- Preferred plate:
  `pipeline-experiments/idea-bx-e2-murphy-farm-bounded-roof-boundary-fix.png`.
- Pipeline steps plate:
  `cartographic-comparisons/bx-murphy-farm-pipeline-steps.png`.

## Result

E1 is the direct map/control/style recipe output. It successfully interprets
the west-side texture as bog-edge terrain and keeps readable plank doors, but it
adds a small roof-nub/chimney artifact and keeps more stone-wall material than
the Roscommon hedge/bank/ditch prior supports.

E2 is a single bounded correction from E1. It removes the roof nub, keeps doors,
preserves the bog-edge west side, and softens the boundaries toward mixed
hedgebanks, banks, ditches, remnant hedges, stone-earthen banks, and short
irregular fieldstone patches. Treat E2 as the preferred Murphy base plate.

## Caveats

- The source detector found zero building-like components in this crop, so the
  deterministic blockout is not building truth.
- The road/lane geometry is plausible and playable, but BX is not a strict
  cartographic-proof pass.
- E2 is a bounded edit, not direct one-shot recipe evidence. Keep E1 when
  judging raw pipeline transfer, and use E2 when judging the candidate artwork.
