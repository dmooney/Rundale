# Style Crops

Small reference crops for image-generation experiments. Use these only for
style, material, camera, and scale cues; the historic map/control images remain
the layout authority.

## Recommended

- `illustrated-style-low-camera-slate-single-house-door-clean.png` — cleaned
  single-building slate/limewash crop with one readable doorway/threshold, no
  chimney, and no partial secondary buildings. Best current reference for slate
  roof, facade, doorway, limewash, stone, and low-camera cues.
- `illustrated-style-low-camera-thatched-single-house-door-clean.png` —
  cleaned single-building thatch variant with one readable doorway/threshold,
  no chimney, and no secondary/partial buildings. Best current reference for
  rough thatch/no-chimney roof behavior.
- `illustrated-style-field-wall-no-animals.png` — cleaned wall/field material
  crop with animal leakage removed.
- `illustrated-style-wall-roof-no-props.png` — cleaned roof/wall material crop
  with prop leakage removed.

## Use With Caution

- `illustrated-style-low-camera-building-clean.png` — useful rough low-camera
  building crop, but the main centered threshold is less explicit than the
  `*-door-clean.png` version. Prefer the door-clean variant.
- `illustrated-style-low-camera-building-door-clean.png` — the main centered
  slate-roof house has a readable doorway/threshold, but the crop still
  includes partial foreground/background building fragments. Do not use it as a
  general reusable style reference; the model may learn that visible doorless
  building fragments are acceptable.
- `illustrated-style-low-camera-thatched-door-clean.png` — leaky intermediary:
  the main house has a door and no chimney, but a partial foreground/edge
  building remains and lacks a readable threshold. Do not use as a general
  style reference; prefer the single-house version.

## Rejection Rules

Reject any crop as a reusable reference if it includes labels, UI, visible
text, people, animals, carts, smoke, chimneys, churches, bridges, water, loose
props, or partial foreground/background houses that could teach the model the
wrong semantics. Door audit is per-building, not per-image: every visible
walkable house fragment needs a readable doorway/threshold, or the crop must be
reduced to exactly one complete building.
