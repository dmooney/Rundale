# Cycle AP2 Kilteevan AO Roof Stub Cleanup Retry Prompt

## Input Roles

- Image 1: full edit target, `docs/graphics-v2/pipeline-experiments/idea-ap-kilteevan-ao-roof-stub-cleanup.png`.
- Image 2: zoom crop reference for the upper-compound roof artifact, `/tmp/rundale-ap-crops/ap-upper-compound-roof-crop-zoom.png`.
- Image 3: zoom crop reference for the lower-left slate-roof artifact, `/tmp/rundale-ap-crops/ap-lower-left-roof-crop-zoom.png`.

## Prompt

```text
Use case: historical-scene
Asset type: bounded precise-object edit of a 16:9 game background plate
Input images and roles:
- Image 1: full edit target, the Cycle AO/AP illustrated isomorphic rural Irish background plate. Edit this image only and keep the same crop, aspect ratio, composition, camera angle, style, and plate content.
- Image 2: zoom crop reference showing the exact remaining upper-compound roof artifact to remove; do not use as the output crop.
- Image 3: zoom crop reference showing the exact remaining lower-left roof artifact to remove; do not use as the output crop.

Primary request:
Remove the remaining visible chimney/stub artifacts only. Preserve the entire scene otherwise.

Critical problem references:
Image 2 is a zoom crop of the upper-compound building. Remove the small chimney/stack protruding from the left/back roof edge of the larger slate-roof house. Repaint the roof edge/ridge with matching slate and ink texture.
Image 3 is a zoom crop of the lower-left slate-roof building. Remove the prominent rectangular chimney on the roof ridge. Repaint the roof underneath with matching slate tiles, ridge line, ink, and watercolor grain.

Also remove any other similar roof/wall protrusion visible in Image 1: chimneys, chimney-like stacks, vents, pipes, roof nubs, wall stacks, capstones, smoke-hole marks, roof posts, or isolated vertical protrusions.

Preserve exactly:
Composition, crop, camera angle, north-up ground plan, open-field softness, muddy roads, central building cluster, upper compound, planted garden/orchard, northeast scrub mass, building footprints, roof materials, walls/hedges/gates, doors, thresholds, windows, trees, colors, paper grain, and illustrated notebook style.

Do not add, move, remove, or redesign buildings, roads, fields, walls, hedges, trees, gardens, doors, thresholds, or windows.
Do not reintroduce continuous stone walls into open fields.
No smoke, roof holes, black puffs, vertical pegs, UI, labels, text, people, animals, carts, barrels, church, graveyard, water, bridge, shopfront, signposts, fog, weather effects, photorealism, 3D, or vector look.

Output:
One repaired plate. This is bounded visual cleanup, not direct one-shot recipe evidence.
```
