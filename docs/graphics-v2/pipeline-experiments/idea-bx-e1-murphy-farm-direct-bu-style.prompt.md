Use case: historical-scene
Asset type: Murphy's Farm direct BU-style background plate, native 16:9
desktop, no UI

Primary request:
Create Cycle BX E1: one neutral-daylight exterior background plate for
Murphy's Farm using the established Graphics V2 pipeline. The result should be
a playable low 3/4 orthographic illustrated game background, not a UI screen
and not a survey-map render.

Input images and roles, in the order shown immediately before generation:

- Image 1 / source map: Murphy's Farm z17 historic OS crop. Highest authority
  for the local road/field/boundary context and north-up orientation. Ignore
  printed labels, survey numbers, and paper artifacts as in-world objects.
- Image 2 / topology material control: deterministic soft-planting control.
  Soft aid only. It helps identify road/field/boundary/planting texture, but
  it is not building truth and must not cause printed letters/numbers to become
  objects.
- Image 3 / camera cue: deterministic oblique raw warp. Use only for the lower
  3/4 pitch/crop feeling. Do not copy beige margins, strip composition, or map
  paper texture.
- Image 4 / style target: BU E2 concept-realism target. Use for warm worn
  paper, rough ink, muddy road scumble, stained limewash, handmade texture, and
  sparse rural realism. Do not copy Beechwood's connected compound topology.
- Image 5 / slate door reference: fitted plank door and limewashed/slate
  facade discipline only.
- Image 6 / thatch door reference: thatched/no-chimney roof and fitted plank
  door discipline only.
- Image 7 / boundary material reference: Irish boundary/wall reference sheet.
  Use only for regional boundary material: hedgebanks, banks, ditches,
  stone-earthen banks, and occasional irregular dry-stone sections. Do not copy
  sky, modern house, horizon, or photo lighting.

World content target:
Murphy's Farm is a working rural Roscommon farm. Show a whitewashed farmhouse,
small stone outbuildings, thatched and/or dark thatch/slate farm roofs where
appropriate, an open muddy working yard, a lane/road connection, hedged fields,
rough farm boundaries, and peat bog / bog-edge terrain west of the farm where
the source map has dense texture. Cattle should not be visible in the base
plate; they belong in runtime animal layers.

Map and topology constraints:

- Keep geographic north at the top of the final image; do not rotate the
  ground plan for a prettier diagonal.
- Use the small farmstead/yard mark near the source crop's center-left as the
  farmstead anchor.
- Treat the textured area west/left of the farmstead as peat bog or bog-edge
  ground: rough dark wet turf, heather/grass texture, drainage cuts or turf-bank
  hints only if they remain subtle and do not become extra roads or walls.
- Preserve the broad diagonal road/field-boundary context as a road/lane edge
  and surrounding fields, but simplify printed map marks into plausible terrain
  only where source-supported.
- Printed labels/numbers such as `130`, `138`, `O`, `Oa`, `N`, and repeated
  survey marks are not world objects.
- Do not invent water, bridge, church, graveyard, shop, people, animals, carts,
  smoke, labels, signs, UI, or extra landmark buildings.

Camera/scale:

- Low 3/4 orthographic game camera, closer playable crop around the farmstead,
  road/lane, yard, and immediate fields.
- Show readable facades and thresholds. Main doors should be roughly in the
  concept-art/BV/BU door-height family, not tiny map ticks.
- Keep roads, yards, gates, thresholds, and paths open for character movement.
- The final plate should feel like a neutral-day base layer for later runtime
  time/weather/season filters.

Door and roof discipline:

- Every visible person-sized opening on a walkable facade must contain a fitted
  wooden plank door and a threshold/step. No black doorway voids.
- Avoid decorative chimneys, roof nubs, roof posts, smoke, ridge stacks, and
  vertical masonry protrusions. The base plate should have no active smoke.

Regional Roscommon boundary treatment:

- Do not turn every boundary into a stone wall.
- Ordinary field and garden divisions should usually be hedges, hedgebanks,
  banks, ditches, remnant hedges, or stone-earthen banks.
- Use exposed irregular stones only in short supported sections: gate ends,
  yard edges, road-edge patches, and stone-earthen bank faces.
- Full dry-stone walls should be rare, broken, irregular, gap-rich, and
  mortarless if present.
- Avoid uniform rectangular blocks, identical gray beads, tidy cobblestone
  strips, continuous wall grids, estate/castle ashlar, and smooth cut masonry.

Style:
Warm hand-inked watercolor parish notebook realism. Use sepia/brown-black ink,
paper grain, dirty limewash, rough gray fieldstone, muted moss/olive greens,
ochre mud roads, patchy grass, weeds, moss, and handmade irregularity. Keep the
image readable and not over-dark.

Success:
The result should be recognizably Murphy's Farm: a whitewashed working farm
cluster anchored to the source-map farmstead area, with open yard and lane,
readable doors, Roscommon hedgebanks/banks/ditches rather than a wall grid, and
the BU E2 concept-realism finish.
