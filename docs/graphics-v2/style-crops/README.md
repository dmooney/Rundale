# Style Crops

Small reference crops for image-generation experiments. Use these only for
style, material, camera, and scale cues; the historic map/control images remain
the layout authority.

## Recommended

- `illustrated-style-low-camera-slate-single-house-door-fixed.png` — repaired
  single-building slate/limewash crop with one readable doorway/threshold, no
  chimney, and no partial secondary buildings. Best current reference for slate
  roof, facade, doorway, limewash, stone, and low-camera cues. Supersedes the
  `*-door-clean.png` source because the repaired version has visible timber
  planks inside the doorway instead of a dark void.
- `illustrated-style-low-camera-thatched-single-house-door-fixed.png` —
  repaired single-building thatch variant with one readable plank
  doorway/threshold, no chimney, and no secondary/partial buildings. Best
  current reference for rough thatch/no-chimney roof behavior. Supersedes the
  `*-door-clean.png` source because the repaired version has a visible fitted
  wooden door.
- `illustrated-style-low-camera-thatched-door-fixed.png` — repaired wider
  thatched crop with plank doors on the visible walkable facades. Useful when a
  wider low-camera thatch/vegetation reference is needed; still prefer the
  single-house fixed crop for safest reusable prompting.
- `illustrated-style-field-wall-no-animals.png` — cleaned wall/field material
  crop with animal leakage removed.
- `illustrated-style-wall-roof-no-props.png` — cleaned roof/wall material crop
  with prop leakage removed.

## Use With Caution

- `illustrated-style-low-camera-building-door-fixed.png` — repaired wider
  slate/thatch building crop with fitted plank doors, but still includes partial
  foreground/background building fragments. Use only when that wider reference
  is specifically needed.
- `illustrated-style-low-camera-building-door-fixed-from-clean.png` — repaired
  variant from the older `building-clean` source. It passes the door audit, but
  remains broader and less isolated than the single-house fixed crops.

## Superseded Dark-Void Crops

These source crops are preserved for provenance, but should not be used as
reusable style references because a dark doorway/opening can teach the image
model to return buildings without visible doors:

- `illustrated-style-low-camera-thatched-door-clean.png`
- `illustrated-style-low-camera-thatched-single-house-door-clean.png`
- `illustrated-style-low-camera-slate-single-house-door-clean.png`
- `illustrated-style-low-camera-building-door-clean.png`
- `illustrated-style-low-camera-building-clean.png`

The door-only repair notes and independent judge verdict are in
`door-fix-cycle-2026-06-28.md`.

## Rejection Rules

Reject any crop as a reusable reference if it includes labels, UI, visible
text, people, animals, carts, smoke, chimneys, churches, bridges, water, loose
props, or partial foreground/background houses that could teach the model the
wrong semantics. Door audit is per-building, not per-image: every visible
walkable house fragment needs a readable doorway/threshold, or the crop must be
reduced to exactly one complete building.
