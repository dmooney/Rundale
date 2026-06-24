# visual-diegetic-travel-cue-m11

The Crossroads hover chevrons were clear but too much like an overlay control.
For this art direction, clickable movement should feel like the world inviting
the player to move: wet road shine, a soft route accent, or a small glint at
the path edge.

## Approach

- Keep the direct action prompt text; it is the accessibility and clarity
  layer.
- Replace travel-chevron graphics with a low-opacity path shimmer drawn inside
  the hotspot bounds.
- Remove broad hover washes that tint whole chunks of the art.
- Feather any obvious hard alpha edges on large local Crossroads atom crops
  found while scrutinizing the hover screenshot.
- Keep inspect cues as bracket corners so object inspection stays visually
  different from movement.
- Avoid adding SVGs or new placeholder art.

## Risk

The cue can become too subtle. The action prompt carries explicit feedback,
so the visual cue can be atmospheric, but it still needs to be perceptible on
desktop hover screenshots.
