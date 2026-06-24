# visual-crossroads-reusable-kit-m10

This milestone answers the user's concern that a beautiful Crossroads screen
can still be only a sliced full render. M9 proved local object placement; M10
must prove reuse. The smallest useful proof is a kit of transparent raster
atoms, repeatedly placed by the scene layer compositor.

## Scope

- Add a `kit/` subdirectory under the Crossroads atoms for small reusable PNG
  pieces.
- Prefer one high-readability family: road puddle/wetness details are ideal
  because they can be stamped several times without breaking architecture,
  perspective, or edge alignment.
- Add multiple `SceneState.layers` entries referencing the same kit assets at
  distinct positions and z-orders near the road surface.
- Keep existing local crops for continuity. This milestone is about proving
  reusability without destroying the art direction.

## Non-goals

- No schema change unless the existing layer contract cannot express reuse.
- No SVG placeholders.
- No full replacement of Crossroads with a procedural/tile map.
- No final asset pipeline claims; this is the first reusable-kit proof.

## Risk

Repeated stamps can look artificial. Use subtle opacity, small scale changes,
and road-surface placement so the repeated atoms read as hand-painted detail
rather than obvious duplicates.
