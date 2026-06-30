# Map To BU-Style Reproducible Pipeline

## Purpose

This pipeline turns the BU concept-realism result into a repeatable Graphics V2
map-to-background process. The first validation target is Grove because it has
a different topology from Beechwood: separate buildings around a working yard
rather than a connected compound.

## Principle

The pipeline should not depend on location-specific prose hints or a previous
render of the same location as an edit target. It may use reusable artifacts:

- source historic map crop,
- reproducible local topology/control crop,
- deterministic oblique camera cue,
- generic door-fixed style references,
- BU E2 as the current material/style target.

## Steps

1. Pick the playable crop before rendering.
   The crop should cover the local building-yard-garden-road core, not an
   arbitrary historic-map extent.

2. Preserve source authority.
   Keep the original historic map visible as feature-existence veto. Map text,
   paper texture, survey dots, and admin/dotted boundaries are nonphysical
   unless corroborated by roads, walls, hedges, buildings, watercourses, or
   gates.

3. Use a topology/control crop.
   The control defines building separation/connection, yards, gardens, roads,
   hedges/walls, gates, tree masses, and exits. It is a layout authority, not a
   style reference.

4. Use an oblique cue only for camera.
   The cue provides low 3/4 orthographic pitch and facade direction. Do not copy
   margins, strip composition, paper texture, or survey-board color.

5. Gate scale by doors.
   Main doors should match the concept-art/BS E2 door-height range. Every
   visible person-sized opening on a walkable facade needs a fitted plank door
   and threshold; black voids fail.

6. Apply BU E2 realism last.
   Use warm worn paper, rough ink, muddy scumbled roads, stained limewash,
   handmade walls/gardens, weeds, moss, sparse practical clutter, and no
   repeated bucket/barrel pattern.

7. Stop after one bounded correction.
   If the first render is close but has one concrete failure, make one targeted
   edit. If topology or doors broadly fail, revise the pipeline inputs rather
   than entering an open-ended polish loop.

## Grove BV Validation

Cycle BV E1 tests this pipeline with:

- `pipeline-experiments/idea-ae-grove-core-control.png`
- `pipeline-experiments/idea-ae-grove-core-control-oblique-raw-warp.png`
- `grove-map-target-site-crop.png`
- `pipeline-experiments/idea-bu-e2-bu-e1-concept-realism-final-tighten.png`
- `style-crops/illustrated-style-low-camera-slate-single-house-door-fixed.png`
- `style-crops/illustrated-style-low-camera-thatched-single-house-door-fixed.png`

Prompt:

- `pipeline-experiments/idea-bv-e1-grove-reproducible-bu-style.prompt.md`

Results:

- `pipeline-experiments/idea-bv-e1-grove-reproducible-bu-style.png` is the
  direct recipe proof. It transfers BU-style realism to Grove while preserving
  separate buildings and doors, but remains slightly clean/high.
- `pipeline-experiments/idea-bv-e2-grove-bv-e1-bu-style-tighten.png` is the
  preferred visual result after the one allowed bounded correction.
- `cartographic-comparisons/bv-grove-reproducible-pipeline-comparison.png`
  compares the map, control, BU E2 target, BV E1, and BV E2.

## Acceptance Gates

- Grove's separate-building yard topology survives.
- The final plate does not copy Beechwood's connected compound layout.
- Major road/yard/garden relationships remain auditable against the map/control.
- Every visible walkable building has a fitted plank door and threshold.
- The style reads as BU E2 concept-realism: warm, worn, handmade, muddy,
  readable, and sparse rather than over-cluttered.
- No UI, labels, people, animals, smoke, water, bridge, church, graveyard,
  shop, signs, or extra source-unsupported structures appear.

## Current Status

Passed on Grove with one bounded correction. BV E1 is the direct pipeline
evidence; BV E2 is the better final plate. Before batch use, validate the same
prompt shape on at least one unrelated third location and make the topology
control-generation step more explicit than reusing an earlier generated AE
control.
