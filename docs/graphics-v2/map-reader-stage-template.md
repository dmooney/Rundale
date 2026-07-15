# Reproducible Map Reader Stage

This is the current preferred way to add detailed building and feature
interpretation without hand-coding special cases for a location.

The map reader is a first-class pipeline stage, not a placeholder for a future
computer-vision pass. It is reproducible because every location gets the same
clean-context prompt, the same input roles, the same output schema, and a saved
note file that can be audited or regenerated.

## Contract

- Run the map reader in a fresh context or subagent.
- Attach only the target historic map crop and this generic rubric.
- Do not attach previous renders, failed attempts, UI concepts, project lore, or
  hand-authored notes.
- Treat the output as observations with confidence, not as truth.
- Save the note file beside the render outputs.
- The render stage may use the note file as soft disambiguation, while the map
  crop remains the primary layout/content evidence.

## Map Reader Prompt

```text
You are a clean-context map-reading worker for a generic historic-map-to-isometric-background pipeline. Do not use prior conversation context. Do not inspect project docs. Use only this instruction and the attached map crop.

Task: produce a data-derived building/feature interpretation note for the attached historic map crop. Do not generate an image. Do not invent lore. Do not write instructions that depend on the site name. Your notes must be produced by this uniform rubric and should be usable for any future location.

Generic rubric:
- Treat the image top as north; describe locations by relative plate position: north edge, northeast quadrant, center-left, etc.
- Ignore printed labels, large letters, survey numbers, modern overlay marks, and paper texture as in-world objects.
- Broad pale corridors with parallel edges or open width are roads/lanes. Single thin lines are more likely walls, hedges, ditches, plot boundaries, or overgrown boundaries than paths.
- Dotted, pecked, dashed, or dot-chain lines may be non-physical
  administrative or survey boundaries such as townland, parish, barony,
  county, estate, or parcel boundaries. If they lack independent physical
  evidence, classify them as administrative/survey/non-physical or ambiguous
  and tell the renderer to ignore them. Do not turn them into hedges, bushes,
  walls, fences, ditches, roads, paths, tree rows, or crop rows. Only classify
  them as physical when corroborated by tree/hedge symbols riding the line,
  double road edges, wall/ditch hatching, enclosure continuity, gate/yard
  relationships, or another physical map mark.
- Dark solid or hatched rectangles are likely buildings or roofed structures. Larger rectangles integrated with yards/gardens/road frontage are probable houses or primary buildings. Small detached rectangles near a yard or primary building are probable sheds, barns, stables, byres, privies, or farm outbuildings. Use confidence, not certainty.
- Enclosed areas subdivided into regular beds or filled with repeated marks are likely gardens, orchards, nurseries, planted yards, or formal grounds.
- Clusters of round/tree symbols are trees, scrub, orchard, woodland edge, or hedgerow planting depending on density and context.
- Identify churches only if there is a clear church/churchyard symbol, label, cross, graveyard-like enclosure, or ecclesiastical footprint in the crop. Otherwise explicitly say no church evidence.
- Identify shops only if there is a clear commercial label/symbol or strong map evidence; otherwise do not classify as shop.
- Identify water only if clear water hachure, stream/river linework, pond shape, or bridge/water-crossing evidence appears. Otherwise explicitly say no water evidence.
- Include uncertainties and alternate interpretations. Do not convert uncertainties into hard constraints.

Output format:
# Data-Derived Map Reader Notes

## Scope
One paragraph stating that notes are derived only from the attached map crop using the generic rubric.

## Orientation And Major Corridors
Bullets for roads/lanes/boundaries, with confidence.

## Building Inventory
A table with columns: ID, Relative position, Shape/footprint, Map evidence, Probable function, Confidence, Notes for renderer. Number buildings B1, B2, etc. Include likely non-building enclosures separately rather than as buildings.

## Enclosures, Planting, And Boundaries
Bullets for gardens/orchards/fields/walls/hedges/ditches, with confidence.

## Explicit Negative Evidence
Bullets for church/shop/water/bridge/text/UI/smoke/etc when absent or uncertain.

## Prompt Insert
A compact neutral paragraph that can be inserted into a generic image prompt. This must be observational, not imperative, and must include uncertainty language.
```

## Render Prompt Template

```text
Use case: historical-scene
Asset type: game environment background plate, 16:9 desktop, no UI

Pipeline inputs:
Image 1 is the target historic map crop and remains the primary layout/content evidence.
Images 2-3 are tiny style/material swatches only.
The attached map-reader notes are the output of a reproducible upstream stage: same rubric, clean context, same input map crop, confidence-graded observations. Treat those notes as soft disambiguation of the map, not as lore and not as override instructions.

Primary request:
Create one finished illustrated background plate for a historical isometric game from Image 1 and the data-derived map-reader notes. Preserve the map crop's physical arrangement before beautifying. The map controls topology; the notes help interpret building footprints, likely functions, uncertainty, and negative evidence.

Reference image rules:
Image 1 controls physical content and placement.
Images 2-3 control only brushwork, ink line, watercolor texture, roof/wall rendering, wall/hedge/tree rendering, terrain palette, and handmade finish.
Do not copy or import objects, people, animals, carts, signs, labels, UI marks, named places, landmarks, churches, graveyards, bridges, rivers, shops, or whole-scene compositions from the style swatches.

Map-reader note rules:
Use building IDs, footprint descriptions, likely function, and confidence language to keep buildings shaped and placed close to the map evidence.
High-confidence observations may influence the render strongly.
Medium-confidence observations should appear plausible but not overly specific.
Low-confidence observations should be omitted or kept ambiguous.
Do not turn uncertainty into hard truth.
Do not add churches, shops, water, bridges, smoke, carts, people, livestock, text, signs, or props unless the notes and map provide clear evidence.

Style/medium:
Hand-inked watercolor over parchment, sepia ink, visible pen hatching, muted moss and olive greens, cream limewashed walls, gray slate or dark thatch where plausible, ochre-brown matte mud roads, soft uneven grass washes, readable 2.5D game-board terrain, crisp but handmade outlines.

Composition/framing:
Make a local playable background plate around the centered or visually dominant building group/site, not a regional overview. Keep north up: source-map top remains final-image top, east right, south bottom, west left. Do not rotate the ground plan for a prettier diagonal composition.

Camera:
Fixed 3/4 orthographic isometric/isomorphic game camera, low oblique pitch around 20-30 degrees downward from horizontal. Show rooftops plus readable vertical facades, doors, thresholds, yards, gates, and walls. Keep all walkable surfaces on one stable ground plane. No horizon, no sky, no vanishing point, no drone view, no steep bird's-eye survey view.

Source-map fidelity:
Paint the map; do not redesign it into a tidier farmstead. Do not consolidate multiple mapped buildings into one neat farmhouse. Do not split one mapped building into many decorative buildings. Keep building footprints near their mapped positions and preserve rough relative sizes and orientations. Preserve visible road corridors, junctions, exits, yards, gates, planted enclosures, field boundaries, walls, hedges, ditches, and overgrown walls aligned with source linework. Printed labels, large letters, survey numbers, and paper texture are ignore marks.

Administrative/survey boundary handling:
Dotted, pecked, dashed, or dot-chain boundaries that the notes mark as
administrative, survey, non-physical, or ambiguous must disappear into the
field texture. Do not render them as hedges, bushes, walls, fences, ditches,
paths, roads, tree rows, crop rows, or any continuous in-world object unless
the map and notes also identify independent physical evidence for that feature.

Walkability:
Roads, lanes, yards, gates, entrances, and thresholds visible in the crop must stay continuous and unobstructed. Do not invent a web of new paths. Where a path crosses a boundary, use a gate/opening rather than making the wall and path collide.

Hard constraints:
No UI, no labels, no signs, no map pins, no visible text, no copied survey numbers, no smoke, no fog, no invented water unless the map clearly shows water, no bridges unless the map clearly shows water crossing, no churches or graveyards unless the map and notes clearly show church/churchyard evidence, no freestanding random chimneys, no chimneys embedded in walls, no decorative roof stacks unless coherent on a rendered building. Base environment layer only.
```

## Review Notes

The map-reader note can be location-specific because it is generated from that
location's map crop, but the procedure must not be location-specific. Do not
manually add a road graph, building lore, named-function guesses, or corrections
that did not come from the same rubric.

Confidence language matters. It lets the renderer emphasize high-confidence
building geometry while keeping ambiguous marks generic or omitting them.
