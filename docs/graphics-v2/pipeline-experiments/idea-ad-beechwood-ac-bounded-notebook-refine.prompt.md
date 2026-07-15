Use case: historical-scene
Asset type: bounded style/camera refinement of an illustrated parish-notebook game background plate, native 16:9 desktop, no UI

Input images and roles:
Image 1: current direct-control background plate to refine. This is the edit target. Preserve its crop, north-up ground plan, building count, building adjacency, yards, road exits, wall/hedge boundaries, garden/enclosure placement, gates, and tree masses.
Image 2: tight local top-down topology control crop. Topology authority. Use it to check that Image 1's roads, buildings, walls, yard/courtyard, garden/enclosure boundaries, gates, exits, and tree masses do not move or change.
Image 3: original historic map crop. Source evidence only. Use it to avoid invented features and semantic leakage.
Image 4: original illustrated parish notebook scene. Style/camera/texture target only: lower playable 3/4 orthographic feel, dense rough hand ink, uneven watercolor washes, muddy lived-in roads/yards, varied vegetation, irregular stone walls, readable facades. Do not copy its UI, people, animals, church, graveyard, bridge, river, labels, signs, carts, smoke, chimneys, or scene layout.
Image 5: cleaned single-building slate/limewash style crop. Door/facade/threshold/roof texture/camera reference only.
Image 6: cleaned single-building thatched/no-chimney style crop. Thatch/no-chimney/door/facade/threshold reference only.
Images 7-8: material swatches only for rough stone, roof/wall texture, muddy ground, fields, grass, and ink/watercolor texture.

Primary request:
Refine Image 1 in place into a more convincing original parish-notebook background plate while preserving its map-derived topology. This is a conservative style/camera/texture repair, not a new layout generation. Do not redraw the scene from scratch.

Absolute topology locks:
Keep the same local plate area and north-up relationships as Image 1. Keep every building in the same location, same connected-or-separated relationship, and same broad footprint. Keep the same road entries/exits and road continuity. Keep the same yard/courtyard/open working areas. Keep garden/enclosure walls and plot boundaries in the same locations. Keep gates and tree masses in the same approximate locations. Do not add, delete, merge, split, rotate, or relocate buildings. Do not add or remove roads, paths, walls, water, bridges, churches, graveyards, shops, people, animals, carts, signs, labels, smoke, or chimneys.

Allowed changes:
Make the plate feel less clean, less regular, less strategy-board-like, and closer to the original illustrated parish notebook art. Add hand-painted irregularity, not new content. Roughen overly perfect garden rows into uneven cultivated beds, patches, weeds, and broken growth while keeping the same enclosed garden areas. Make stone walls less bead-like and more irregular in size, lean, gaps, moss, weeds, and watercolor bleed while keeping their alignment. Break up overly uniform roof grids with hand-drawn uneven slate/thatch strokes, stains, moss, edge wear, and warped hand-built geometry without changing roof footprints. Make road and yard surfaces muddier, more scumbled, more varied, with ruts, stones, damp patches, and soft watercolor transitions. Increase paper tooth, ink wobble, wash granulation, and muted earth/grass variation.

Camera/style refinement:
Keep orthographic isomorphic gameplay usability and north-up orientation. If possible without moving topology, make the view feel a little lower and more human-scale: facades, side walls, door faces, thresholds, wall side faces, gate posts, and tree lower masses should read more strongly relative to roof planes. Roofs must remain visible, but the scene should not feel like a high survey board. No horizon, no sky, no vanishing-point perspective, no fisheye, no rotated ground plan.

Door/facade rule:
Every visible playable building facade facing a road, yard, lane, garden entry, or courtyard must retain or gain a clear dark doorway or threshold unless that facade is truly cropped off-frame or fully occluded by an unchanged source-faithful object. Any visible foreground, background, or edge building with no readable door is a failure. Doors should be simple dark timber openings or plain plank doors integrated into limewashed or stone walls, with small worn thresholds or muddy approaches.

Building translation:
Preserve humble early-19th-century rural Irish vernacular: rough limewashed stone, patched plaster, irregular hand-built geometry, low eaves, damp stone bases, weathered thatch where appropriate, rough slate where appropriate, simple dark door openings, small dark windows, uneven thresholds. Many poor period dwellings had no chimneys; use no visible chimneys and no smoke. Remove or avoid random freestanding chimneys, chimneys embedded in walls, chimney-like roof nubs, and chimney-like wall projections.

Hard negatives:
No UI, labels, border, characters, people, animals, carts, loose props, visible text, shop signs, chapel/church/graveyard cues, water, bridges, smoke, fog, modern details, fantasy buildings, polished estate architecture, perfect garden grids, identical stone beads, clean vector outlines, glossy concept-art lighting, 3D render look, mobile strategy-board neatness, or any visible building facade without a readable threshold.

Final output:
One native 16:9 PNG background plate. It should be directly comparable to Image 1 for identical topology and closer to Image 4/Images 5-8 for lower notebook-style camera, rough ink, uneven watercolor, muddy surfaces, and human-scale detail.
