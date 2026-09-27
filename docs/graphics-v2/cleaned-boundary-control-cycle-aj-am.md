# Cleaned Boundary Control Cycle AJ/AM

## Purpose

Test whether a reproducible pre-cleaned map/control image can reduce the
recurring failure where the render model turns dotted or pecked administrative
and survey linework into physical walls, hedges, tracks, or planted rows.

This cycle remains data-driven: no hand-authored location-specific road,
building, wall, or landmark hints are added. The original map crop stays the
source of truth, while the cleaned crop is used only to de-emphasize likely
non-physical dot chains.

## Control Prep

Prototype tool:
`scripts/suppress_dot_chains.py`

Source crop:
`map-sources/kilteevan-z17-map-crop.png`

Accepted render input:

- `pipeline-experiments/idea-aj2-kilteevan-dot-suppressed-no-admin-map-crop.png`
- `pipeline-experiments/idea-aj2-kilteevan-dot-suppressed-no-admin-oblique-raw-warp.png`
- `pipeline-experiments/idea-aj2-kilteevan-dot-suppressed-suppression-overlay.png`

Rejected control variants:

- `idea-aj-kilteevan-dot-suppressed-*` was too broad; it removed too many
  tree/symbol marks and weakened source evidence.
- `idea-aj3-kilteevan-dot-suppressed-strip-*` added continuous chain corridors
  but over-erased central linework.
- `idea-aj4-kilteevan-dot-suppressed-mainchains-*` was less destructive than
  AJ3, but still left artificial diagonal corridors through important central
  evidence.

## Render Test

Cycle AM uses the original map crop, the AJ2 cleaned crop, the AJ2 oblique
camera cue, and cleaned low-camera material/style crops. The prompt explicitly
states that soft erased seams in the cleaned crop are deletion artifacts, not
physical terrain.

Output:

- `pipeline-experiments/idea-am-kilteevan-aj2-cleaned-control-direct.png`
- `pipeline-experiments/idea-am-kilteevan-aj2-cleaned-control-direct.prompt.md`
- `pipeline-experiments/idea-am-kilteevan-aj2-cleaned-control-direct.report.md`

## Result

Cycle AM is a useful partial pass. The bold deleted diagonal admin/dot-chain
boundary is no longer plainly restored as one continuous wall, hedge, road,
crop row, or tree row. The central road/building group, upper compound,
center-right planted enclosure, and northeast tree mass remain broadly
readable, and the plate has no UI, labels, people, animals, church, water,
bridge, smoke, or obvious chimney stack.

The remaining failure is material hierarchy: the render still turns many thin
or ambiguous field/plot divisions into fairly continuous stone walls. This is
not the same as the original diagonal-dot failure, but it means the recipe is
not yet final. The next prompt/control iteration should distinguish physical
stone walls from softer hedges, ditches, overgrown banks, and uncertain parcel
edges more strongly.

Art style is acceptable but still cleaner and more survey-like than the
original illustrated parish notebook target. The crop scale and building
facades are usable, though the camera remains a touch high.

## Audit Questions

- Does the bold diagonal dotted boundary stop becoming a stone wall, hedge,
  road, crop row, or tree row?
- Does the render avoid tracing the softer erased scars from the cleaned crop?
- Does topology remain close to Cycle AH/AI: central road/building cluster,
  upper compound, center-right planted enclosure, and northeast tree/scrub?
- Does the image keep the illustrated parish notebook look instead of becoming
  a clean survey-board render?
- Are all visible walkable buildings equipped with readable doors and
  thresholds?
- Are chimneys, chimney-like nubs, wall stacks, vents, and smoke absent?

## Audit Answers

- Deleted diagonal boundary: improved; not obviously restored as a single
  terrain feature.
- Erased scars: mostly improved; the render does not copy the main AJ2 scar as
  a direct terrain trace, though broad walling remains heavy elsewhere.
- Topology: partial pass; major map relationships survive.
- Notebook look: partial pass; ink/watercolor style is present, but the result
  remains cleaner and more regular than the original sample.
- Doors: mostly pass at playable scale; small upper-compound doors are less
  readable but not foreground-critical.
- Chimneys/smoke: pass; no obvious chimneys, roof nubs, vents, stacks, or smoke.
