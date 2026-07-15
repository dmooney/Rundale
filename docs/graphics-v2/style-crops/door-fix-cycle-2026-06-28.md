# Door Fix Cycle - 2026-06-28

## Scope

Door-only imagegen repairs for five Graphics V2 style crops flagged as teaching
empty dark doorway voids. These are style-reference repairs only; no map pipeline,
garden, topology, roof, or composition work is included.

## Outputs Under Audit

- `illustrated-style-low-camera-thatched-door-fixed.png` from `illustrated-style-low-camera-thatched-door-clean.png`
- `illustrated-style-low-camera-thatched-single-house-door-fixed.png` from `illustrated-style-low-camera-thatched-single-house-door-clean.png`
- `illustrated-style-low-camera-slate-single-house-door-fixed.png` from `illustrated-style-low-camera-slate-single-house-door-clean.png`
- `illustrated-style-low-camera-building-door-fixed.png` from `illustrated-style-low-camera-building-door-clean.png`
- `illustrated-style-low-camera-building-door-fixed-from-clean.png` from `illustrated-style-low-camera-building-clean.png`

## Acceptance Rule

A style crop is safe only if every visible walkable facade has a real fitted
timber/plank door, not just a dark void. Partial roof-only fragments with no
visible facade are not doorway failures.

## Method

Mode: built-in imagegen edit.

Prompt pattern: edit only existing dark doorway openings; add visible weathered
brown or gray-brown timber plank doors fitted inside the opening with subtle
vertical planks and a threshold; preserve crop, buildings, roofs, walls,
vegetation, camera, paper texture, watercolor/ink style, lighting, and
composition; no props, people, animals, chimneys, smoke, labels, text, UI,
modern details, panels, or collages.

One discarded generated output for `illustrated-style-low-camera-building-door-clean.png`
formed a comparison/collage and was not copied into the repo.

## Independent Judge

Clean-context subagent `019f100f-cf85-7852-afc7-36f9c62cc141` audited the five
fixed variants with the strict "plank door, not dark void" rubric.

- `illustrated-style-low-camera-thatched-door-fixed.png`: PASS. Visible
  walkable doors have fitted wooden plank surfaces and connect to ground/yard
  areas. Bottom-left cropped roof/facade fragment is N/A.
- `illustrated-style-low-camera-thatched-single-house-door-fixed.png`: PASS.
  Main cottage door is clearly planked, fitted, and threshold connects to the
  yard/path. Bottom-left partial edge building is N/A.
- `illustrated-style-low-camera-slate-single-house-door-fixed.png`: PASS. Main
  facade door has visible vertical timber planks and a connected stone
  threshold/path.
- `illustrated-style-low-camera-building-door-fixed.png`: PASS. All visible
  walkable facades show fitted plank doors, including the background house and
  main thatched building. Bottom-left partial crop is N/A.
- `illustrated-style-low-camera-building-door-fixed-from-clean.png`: PASS.
  Visible doors are wooden/planked and fitted; thresholds meet yard/ground.
  Bottom-left partial crop is N/A.
