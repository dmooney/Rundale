Use case: style-transfer
Asset type: historical isomorphic game background plate refinement, 16:9 desktop background plate
Input images:

- Image 1 is the edit target and primary topology/style target: the Beechwood Cycle Q plate. Preserve its ground plan, road positions, compound footprint, garden enclosure, building count, building placement, field boundaries, tree masses, crop, and north-up orientation.
- Image 2 is the topology authority: the Beechwood Cycle M cleaned top-down control. Use it only to veto topology drift. Do not copy its high top-down camera or clean board-game finish.
- Image 3 is the original illustrated parish notebook sample. Use it only for art direction: loose ink, watercolor density, low 3/4 orthographic camera feel, rough stone, muddy roads, varied vegetation, paper grain, and readable facades. Do not copy its UI, people, church, graveyard, river, bridge, signs, shop, labels, carts, animals, smoke, or whole-scene layout.
- Image 4 is a no-UI notebook-style visual target from another map crop. Use it only for rough watercolor texture, muted palette, scumbled roads, and hand-drawn wall/field texture. Do not copy its Grove road layout, building layout, garden layout, or field arrangement.
- Images 5-8 are cleaned style/material swatches. Use them only for slate roof, thatch/no-chimney behavior, limewashed walls, door/threshold rendering, rough field/wall texture, and prop-free roof/wall treatment.

Primary request:
Refine Image 1 toward the original illustrated parish notebook look while preserving the Beechwood Cycle Q/M topology. This is a bounded repaint, not a new scene. Keep the same Beechwood compound and local map geometry, but make the art feel more like a hand-drawn parish notebook plate: lower, rougher, denser, more watercolor, less clean survey tile.

Camera and projection:
Keep north up. Keep the ground plan unrotated. Use a low 3/4 orthographic/isomorphic game camera, camera south of the scene looking north, roughly 30-35 degrees above the ground plane. No horizon, no sky, no vanishing point, no fisheye, no drone/satellite top-down view. The image should still work as a walkable game plate: characters could stand on the roads, yard, garden paths, and thresholds without perspective mismatch.

Topology preservation:
Preserve the exact Beechwood layout from Image 1, checked against Image 2:

- The diagonal road remains in the same position and exits the frame in the same places.
- The connected L/U-shaped compound remains connected, with the same courtyard relationship to the road and garden.
- The lower/foreground building group remains where it is, with the same relationship to the road and small enclosure.
- The large walled garden remains attached to the compound at the same place, with the same broad rectangle and internal path/row structure.
- Tree masses remain in the same broad zones: dense woodland/trees on the left/top-left, open fields where Image 1 has open fields, scattered hedges/walls where Image 1 has them.
- Do not add scenic crossroads, extra paths, extra lanes, extra buildings, water, bridges, church/graveyard features, shops, signs, people, animals, carts, barrels, tubs, loose props, labels, UI, or text.
- Do not convert administrative/survey-like linework into physical walls unless Image 1 already shows it as a physical wall or hedge.

Style target:
Move away from the clean regular board-game look of Image 1. Move toward Image 3's notebook illustration:

- heavier but broken sepia/brown ink outlines,
- loose watercolor washes with visible paper grain,
- muddy scumbled roads with stones, ruts, and uneven edges,
- limewashed walls stained with damp, soot-darkened lower stones, moss, chips, and irregular brush texture,
- slate roofs with broken hand-hatched slate marks, uneven color, and dry-brush stains,
- optional thatched roofs only where a roof already exists and only if it does not change the footprint,
- rough, irregular wall stones rather than perfect continuous wall ribbons,
- garden rows with varied plant blobs and hand-painted soil texture rather than crisp plan-view grids,
- trees with visible lower trunks and dark lower masses, not identical round symbols,
- open fields as mottled watercolor grass with sparse weeds, not empty flat fill.

Lower-camera/facade improvement:
Make building facades more readable than in Image 1 without moving the buildings. Strengthen wall side faces, eaves, thresholds, steps, damp stone bases, and door/window proportions so the plate feels closer to the original notebook sample. The camera should feel a little lower and more human-scale than Image 1, but not so low that the ground plan or walkable surfaces become distorted.

Doors on openings:
Every visible walkable house, cottage, barn, byre, shed, outbuilding, or partial building that shows any facade must have one readable human-usable timber plank door on a visible facade, plus a small threshold connected to a yard, path, or road. If you paint any person-sized dark vertical opening, doorway, entry gap, shed mouth, barn mouth, or black rectangular hole in a wall, put a visible brown or weathered gray-brown timber plank door directly inside that opening, fitted to the opening, with vertical plank marks or a half-open plank slab. Do not leave empty black door holes. Do not imply a doorway with shadow only. Do not place a door beside the opening while the opening remains empty. This includes foreground, background, edge, side, and small outbuildings.

Roof and period rules:
No chimneys, chimney-like roof nubs, vents, roof pipes, freestanding stacks, wall stacks, smoke holes, visible smoke, fog, or steam. Many rural thatched buildings in this period had no chimney; if a roof looks like it needs ventilation, still do not draw a protrusion. Keep roofs low and plain. Do not add modern gutters, glassy windows, metal roofing, telegraph poles, paved roads, vehicles, or modern details.

Boundary/material rules:
Make important outer compound/garden walls readable, but avoid turning every thin line into a perfect stone wall. Where Image 1 has garden/internal rows, show softer planting edges, low beds, rough stakes, patchy soil, or brush texture. Where Image 1 has open fields, keep them open. Wall lines should be broken, irregular, overgrown, and hand-drawn; not clean CAD outlines.

Editing discipline:
This is not a fresh composition. Preserve Image 1's crop, relative scale, building count, roads, yards, garden, tree zones, and field zones. Do not beautify by inventing new scenic geometry. Do not simplify the Beechwood compound into separate cottages, and do not add a Grove-like crossroads. Use Image 2 to catch layout drift, not as visual style.

Output:
A UI-free historical isomorphic game background plate in the original illustrated parish notebook aesthetic, with Beechwood Cycle Q/M topology intact, lower-camera facades, richer watercolor texture, doors fitted into all human-sized openings, no smoke/chimneys/props/semantic leaks, and no text.
