Use case: precise-object-edit
Asset type: historical isomorphic game background plate refinement, 16:9 desktop background plate
Input images:

- Image 1 is the edit target: the Beechwood Cycle BJ repaint. Preserve its Beechwood topology, crop, road exits, connected compound, garden, lower building group, field zones, tree zones, doors, and roof discipline.
- Image 2 is the topology veto: Beechwood Cycle M cleaned top-down control. Use it only to reject layout drift, not as style.
- Image 3 is the original illustrated parish notebook sample. Use it only for art direction: lower readable facades, rougher ink, looser watercolor, paper grain, scumbled roads, irregular vegetation, and hand-painted density. Do not copy its UI, people, church, graveyard, river, bridge, signs, shop, labels, animals, carts, smoke, chimneys, or whole-scene layout.
- Images 4-7 are cleaned material/style swatches for slate/limewash/door, thatch/no-chimney/door, field/wall watercolor, and prop-free wall/roof texture. Use them only as material rendering references.

Primary request:
Make a very conservative refinement of Image 1 that moves it closer to the original illustrated parish notebook look while preserving Beechwood topology. The specific visual goal is: lower-feeling facades, rougher ink-and-watercolor surface, less tidy garden-grid/survey-board regularity, and more hand-painted texture. This is not a fresh render and not a re-layout.

What to improve:

- Make the camera feel slightly lower and more human-scale by strengthening existing visible wall faces, eaves, damp stone bases, thresholds, and door/window proportions. Do this without moving buildings or changing the ground plan.
- Make roads and yards more like the original notebook sample: muddy, scumbled, rutted, uneven-edged, stone-speckled, and hand-painted, while keeping the same road widths and exits.
- Make garden beds less like crisp plan-view rectangles. Keep the same garden footprint and internal path/row organization, but repaint rows as uneven soil, soft planting, low brush, rough stakes, varied plant blobs, and patchy watercolor. Do not add new paths or new walls.
- Make wall lines rougher and more overgrown without making them more continuous, taller, cleaner, or more numerous. Broken, irregular, partially grass-covered walls are preferred.
- Add watercolor mottling, paper grain, dry-brush stains, moss, chips, and broken sepia ink to fields, roofs, walls, and vegetation.

Hard topology constraints:
Preserve Image 1's layout and check against Image 2:

- same diagonal road position and road exits,
- same connected L/U-shaped compound footprint and courtyard,
- same lower/foreground building group and small enclosure,
- same attached rectangular garden footprint and broad internal structure,
- same tree mass zones and open-field zones,
- same crop, north-up orientation, and low 3/4 orthographic/isomorphic projection.
  Do not add scenic crossroads, extra roads, extra lanes, extra paths, extra buildings, extra garden compartments, bridges, water, church/graveyard features, shops, signs, UI, labels, people, animals, carts, barrels, tubs, loose props, or text.

Do not increase walling:
Do not turn texture into new walls. Do not convert thin garden rows, field mottling, brush strokes, or administrative/survey-like traces into stone walls. Do not make existing garden edges taller, cleaner, or more fortress-like. Do not add wall caps around every planting bed. A softer garden is the desired direction.

Doors on openings:
Preserve all existing readable doors. Every visible walkable house, cottage, barn, byre, shed, outbuilding, or partial building that shows a facade must have one readable human-usable timber plank door on a visible facade, plus a small threshold connected to a yard/path/road. If any person-sized dark vertical opening exists, put a visible brown or weathered gray-brown plank door fitted directly inside that opening. Do not leave empty black door holes, and do not place a door beside an empty opening.

Roof and period rules:
Preserve the no-chimney success from Image 1. No chimneys, chimney-like roof nubs, vents, roof pipes, freestanding stacks, wall stacks, smoke holes, visible smoke, fog, steam, modern gutters, shiny glass, metal roofing, telegraph poles, paved roads, vehicles, or modern details. Roof marks must read as flush slate hatching, thatch texture, moss, or stains, not protrusions.

Camera/projection rules:
Keep north up and ground plan unrotated. Use a low 3/4 orthographic/isomorphic camera, camera south of the scene looking north, around 30-35 degrees above the ground plane. No horizon, sky, vanishing point, fisheye, drone view, or top-down plan view. The result must remain walkable for game characters on roads, courtyards, garden paths, and thresholds.

Editing discipline:
Change texture, line quality, facade readability, and material feel. Do not change layout. Do not beautify by inventing new scenic geometry. Do not simplify the connected Beechwood compound into separate cottages. Do not copy Grove or the notebook sample's landmarks. If a requested style improvement conflicts with topology preservation, topology preservation wins.

Output:
A UI-free historical isomorphic game background plate, visibly still Beechwood Cycle BJ/Q/M in layout, but closer to the original illustrated parish notebook: lower-feeling facades, rougher broken ink, richer watercolor paper texture, softer less-survey-like garden rows, doors fitted into all human-sized openings, no chimneys/smoke/props/semantic leaks, and no text.
