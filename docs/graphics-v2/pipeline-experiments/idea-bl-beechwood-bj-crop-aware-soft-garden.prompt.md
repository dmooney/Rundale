Use case: precise-object-edit
Asset type: historical isomorphic game background plate refinement, 16:9 desktop background plate

Input images:
- Image 1 is the full edit target: `idea-bj-beechwood-q-notebook-repaint.png`. Preserve its Beechwood topology, crop, road exits, connected compound, garden, lower building group, field zones, tree zones, doors, and roof discipline.
- Image 2 is the full topology veto: `idea-m-beechwood-admin-topdown-cleaned.png`. Use it only to reject layout drift, not as style.
- Image 3 is the BJ garden audit crop: `idea-bj-audit-garden-regular-bj.png`. This shows the exact visual problem to improve: garden compartments and rows are too crisp, rectangular, repeated, and survey-like.
- Image 4 is the Q garden audit crop: `idea-bj-audit-garden-regular-q.png`. This is the earlier baseline; do not return to its clean tile look.
- Image 5 is the M garden topology crop: `idea-bj-audit-garden-regular-m.png`. This crop preserves the broad garden footprint and internal organization, but it is top-down and diagrammatic. Use it only as a topology veto.
- Image 6 is the BJ compound facade crop: `idea-bj-audit-compound-facades-bj.png`. Preserve the connected compound, readable facades, doors, courtyard, and roof discipline from this crop.
- Image 7 is the BJ lower-building crop: `idea-bj-audit-lower-buildings-bj.png`. Preserve these lower buildings, their doors, the small enclosure, and their relationship to the road.
- Image 8 is the original notebook style crop: `idea-bj-audit-original-style-scene.png`. Use it only for rough ink, muddy scumbled roads, uneven vegetation, watercolor density, paper grain, and lower facade feel. Do not copy any church, shop, people, labels, signs, bridge, water, animals, carts, barrels, UI, or whole-scene layout.
- Images 9-12 are cleaned material/style swatches for slate/limewash/door, thatch/no-chimney/door, field/wall watercolor, and prop-free wall/roof texture. Use them only as material rendering references.

Primary request:
Make a very conservative crop-aware refinement of Image 1. Preserve Beechwood
BJ/Q/M topology, but make the garden and surrounding fields less diagrammatic
and closer to the original illustrated parish notebook look. This is a bounded
edit, not a fresh render and not a re-layout.

The exact problem:
In Image 3, the garden reads too much like a clean plan: hard rectangular beds,
evenly repeated rows, crisp thin outlines, regular path strips, and continuous
wall-like edges. The desired direction is softer and more hand-painted:
uneven soil, broken low planting, low brush, irregular stakes, mixed plant
blobs, patchy watercolor, weeds, and scumbled dirt. Keep the garden footprint
and rough internal organization, but reduce the feeling of a CAD grid or raised
stone-bed diagram.

What to improve:
- Soften garden rows and beds. Keep their broad positions, but break their
  mechanical repetition with varied plant spacing, missing patches, soil
  stains, weeds, and hand-painted irregularity.
- Keep the main garden perimeter and important outer compound walls readable,
  but make wall stones broken, grass-overgrown, uneven, and lower-feeling.
- Do not add new walls. Do not put wall caps around each individual bed. Do not
  turn row texture into masonry.
- Roughen roads and yards with muddy scumbling, stones, rut marks, and uneven
  watercolor edges while keeping the same road widths and exits.
- Strengthen only existing building facades, eaves, damp stone bases,
  thresholds, and door/window proportions enough to feel slightly lower and
  more notebook-like. Do not move or resize buildings.
- Add paper grain, dry-brush stains, moss, chips, broken sepia ink, and varied
  vegetation texture across fields, roofs, walls, and tree masses.

Hard topology constraints:
Preserve Image 1's layout and check against Images 2 and 5:
- same diagonal road position and road exits,
- same connected L/U-shaped compound footprint and courtyard,
- same lower/foreground building group and small enclosure,
- same attached rectangular garden footprint and broad internal organization,
- same tree mass zones and open-field zones,
- same crop, north-up orientation, and low 3/4 orthographic/isomorphic
  projection.

Do not add scenic crossroads, extra roads, extra lanes, extra paths, extra
buildings, extra garden compartments, bridges, water, church/graveyard
features, shops, signs, UI, labels, people, animals, carts, barrels, tubs,
loose props, or text.

Do not increase walling:
This is the most important style constraint. Do not turn texture into new
walls. Do not convert thin garden rows, field mottling, brush strokes, crop
marks, or administrative/survey-like traces into stone walls. Do not make
existing garden edges taller, cleaner, more continuous, or more fortress-like.
Softer garden texture is the goal.

Doors on openings:
Preserve all existing readable doors from Images 6 and 7. Every visible
walkable house, cottage, barn, byre, shed, outbuilding, or partial building
that shows a facade must have one readable human-usable timber plank door on a
visible facade, plus a small threshold connected to a yard/path/road. If any
person-sized dark vertical opening exists, put a visible brown or weathered
gray-brown plank door fitted directly inside that opening. Do not leave empty
black door holes, and do not place a door beside an empty opening.

Roof and period rules:
Preserve the no-chimney success from Image 1. No chimneys, chimney-like roof
nubs, vents, roof pipes, freestanding stacks, wall stacks, smoke holes, visible
smoke, fog, steam, modern gutters, shiny glass, metal roofing, telegraph poles,
paved roads, vehicles, or modern details. Roof marks must read as flush slate
hatching, thatch texture, moss, or stains, not protrusions.

Camera/projection rules:
Keep north up and ground plan unrotated. Use a low 3/4 orthographic/isomorphic
camera, camera south of the scene looking north, around 30-35 degrees above
the ground plane. No horizon, sky, vanishing point, fisheye, drone view, or
top-down plan view. The result must remain walkable for game characters on
roads, courtyards, garden paths, and thresholds.

Editing discipline:
Change texture, line quality, garden softness, facade readability, and material
feel. Do not change layout. Do not beautify by inventing new scenic geometry.
Do not simplify the connected Beechwood compound into separate cottages. Do
not copy Grove or the notebook sample's landmarks. If a requested style
improvement conflicts with topology preservation, topology preservation wins.

Output:
A UI-free historical isomorphic game background plate, visibly still Beechwood
BJ/Q/M in layout, but closer to the original illustrated parish notebook:
lower-feeling facades, rougher broken ink, richer watercolor paper texture,
softer less-survey-like garden rows, doors fitted into all human-sized
openings, no chimneys/smoke/props/semantic leaks, and no text.
