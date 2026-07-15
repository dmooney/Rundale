Use case: historical-scene
Asset type: reproducible map-to-BU-style Grove validation background plate,
native 16:9 desktop, no UI

Primary request:
Create Cycle BV E1, a Grove validation of the reproducible Graphics V2
map-to-background pipeline. Generate the final plate directly from reusable
map/control inputs and generic style targets. Do not use any previous Grove
render as an edit target.

Input images and roles:

- Grove core topology control:
  `idea-ae-grove-core-control.png`. Primary plate-area and layout authority.
  It defines the intended crop: separate buildings around a working yard,
  garden/orchard block to the north/upper-left, local roads, gates, walls,
  hedges, tree masses, and exits.
- Grove core oblique cue:
  `idea-ae-grove-core-control-oblique-raw-warp.png`. Camera/pitch cue only.
  Use it for low 3/4 orthographic facade direction. Do not copy beige margins,
  strip composition, empty borders, or flat map-control texture.
- Grove historic map:
  `grove-map-target-site-crop.png`. Highest authority for feature existence.
  Use it to prevent invented water, bridges, churches, graveyards, extra roads,
  extra buildings, labels-as-objects, or survey text.
- BU E2 Beechwood concept-realism target:
  `idea-bu-e2-bu-e1-concept-realism-final-tighten.png`. Style/material target
  only: warm paper, rough ink, worn limewash, muddy scumbled roads, fitted
  plank doors, sparse practical clutter, handmade walls/gardens, and dense but
  readable watercolor texture. Do not copy Beechwood's connected-compound
  topology or building arrangement.
- Door-fixed single-building slate and thatch references:
  `illustrated-style-low-camera-slate-single-house-door-fixed.png` and
  `illustrated-style-low-camera-thatched-single-house-door-fixed.png`. Use only
  for fitted plank doors, door height, threshold detail, wall/roof material,
  and no-black-void doorway discipline. Do not copy their layouts.

Pipeline being tested:

1. Choose the playable crop/control before rendering.
2. Use the historic map as feature-existence veto.
3. Use the oblique cue for low 3/4 orthographic camera only.
4. Use BU E2 and the door-fixed crops for final material/style/door discipline.
5. Produce the final no-UI plate in one render, with no previous Grove render
   used as an edit target.

Grove topology invariants:

- Preserve Grove's separate-building yard topology. Do not merge the buildings
  into a Beechwood-like connected courtyard compound.
- Preserve the long lower/southern building range, the left/west small building,
  the central smaller yard building, the taller east/right building beside the
  road, and their working-yard relationships as separate readable structures.
- Preserve the garden/orchard block north/upper-left of the yard as planting
  texture and soft enclosure, not a fortress or perfect strategy-game grid.
- Preserve the road curving through the local yard area and exiting the crop.
  Roads and yards stay open, muddy, and walkable.
- Preserve nearby tree/hedge masses and wall/field-edge relationships without
  turning every line into a heavy wall.
- Do not add water, bridges, churches, graveyards, shops, signs, labels, UI,
  people, animals, carts, smoke, chimneys, extra roads, or extra buildings.

Camera and scale:
Use the concept-art/BU branch scale: close playable crop, low 3/4 orthographic
game camera, north-up ground plan, large readable facades, and doors roughly in
the original notebook/BS E2 height range. The plate should feel human-scale,
not like a distant survey board. Roofs remain visible but must not dominate.
No horizon, sky, fisheye, vanishing-point perspective, drone angle, or rotated
ground plan.

Door/facade rule:
Every visible person-sized opening on every walkable facade must contain a
fitted wooden plank door and a small threshold/step. Do not render black voids.
Do not hide doors behind clutter, vegetation, or shadow. If a facade faces the
yard, road, lane, gate, or garden entry, it needs a readable door unless it is
truly cropped off-frame or fully occluded by source-faithful structure.

Material/style rule:
Match BU E2's final concept-realism recipe: warm worn paper, sepia/black ink,
scumbled watercolor, muddy wheel ruts and footpaths, rough stone, stained
limewash, straw-brown thatch where appropriate, weathered slate where
appropriate, moss, lichen, weeds, patchy grass, imperfect garden rows, broken
road edges, and sparse practical yard wear. Keep clutter sparse: at most 2-3
small inert objects total, and no repeated bucket/barrel/crate pattern.

Boundary/path rule:
Road-width corridors remain walkable roads/tracks/yards. Single thin linework
is usually boundary/hedge/ditch/wall/plot edge/vegetation edge, not a new path.
Garden rows and orchard dots are planting/soil texture, not walls or footpaths.
Survey dots, map paper texture, printed labels, and administrative dotted
boundaries leave no physical trace unless corroborated by real features.

Failure conditions:

- Any visible walkable building has only a black doorway void or no door.
- The separate Grove buildings merge into one connected compound.
- The image copies Beechwood's layout instead of Grove's.
- The Grove label or any map text appears in the art.
- The plate adds church/bridge/water/graveyard/shop/signage/people/animals/UI.
- Garden rows become a hard perfect grid or all boundaries become heavy walls.
- The image becomes uniformly dark, grimy, or cluttered.

Success:
One native 16:9 Grove background plate that is directly auditable against the
Grove control/map for topology and against BU E2 for concept-realism. It should
look like BU E2's warm, worn, handmade no-UI art direction applied to Grove's
own separate-building yard topology.
