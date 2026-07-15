# Cycle AP Kilteevan AO Roof-Stub Cleanup Prompt

Input role: Image 1 is the edit target only. Source image:
`/Users/dmooney/Rundale/docs/graphics-v2/pipeline-experiments/idea-ao-kilteevan-open-fields-direct.png`

Tool path: imagegen skill, built-in `image_gen` edit call.

```text
Use case: historical-scene
Asset type: bounded precise-object edit of a 16:9 game background plate
Input images: Image 1 is the edit target only. It is the Cycle AO illustrated isomorphic rural Irish background plate.
Edit target: Image 1, the Cycle AO illustrated isomorphic rural Irish background plate.
Primary request: Remove only chimney/stub/protrusion artifacts from roofs and walls. Preserve the entire scene otherwise.
Remove: every chimney, chimney-like stack, roof nub, vent, pipe, wall stack, capstone, smoke-hole mark, small roof post, and isolated vertical protrusion on any roof or wall.
Repair method: repaint removed spots with matching slate, thatch, limewash, ink linework, and watercolor grain so roof ridges and wall tops look natural and uninterrupted.
Preserve exactly: composition, crop, camera angle, north-up ground plan, open-field softness, muddy roads, central buildings, upper compound, planted garden/orchard, northeast scrub mass, building footprints, roof materials, walls/hedges/gates, doors, thresholds, windows, trees, colors, paper grain, and illustrated notebook style.
Do not add, move, remove, or redesign buildings, roads, fields, walls, hedges, trees, gardens, doors, thresholds, or windows.
Do not reintroduce continuous stone walls into open fields.
Hard avoid: smoke, roof holes, black puffs, vertical pegs, UI, labels, text, people, animals, carts, barrels, church, graveyard, water, bridge, shopfront, signposts, fog, weather effects, photorealism, 3D, vector look.
Output: one repaired plate. This is bounded visual cleanup, not direct one-shot recipe evidence.
```

Implementation note: the built-in edit output was used as bounded donor material only inside small roof masks because the direct edit also softened/repainted the whole plate. The delivered PNG preserves Image 1 outside the explicit cleanup masks.
