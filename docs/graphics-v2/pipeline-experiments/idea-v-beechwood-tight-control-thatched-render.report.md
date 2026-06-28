# Idea V Beechwood Tight Control Thatched Render

- Generated with the built-in `image_gen` tool using the attached local images as references and the exact prompt in `idea-v-beechwood-tight-control-thatched-render.prompt.md`.
- Output copied from `/Users/dmooney/.codex/generated_images/019f0c29-81e5-7220-9e70-3c75779a3413/ig_0f5f6f46d2795e50016a408e1377188196b1e28256ba0217d9.png`.
- Saved PNG: `idea-v-beechwood-tight-control-thatched-render.png`.
- Pixel size: `1672 x 941` PNG, near-native 16:9.
- Leaky style crop check: did not use `illustrated-style-low-camera-thatched-door-clean.png`.

## Verdict

Strong topology-control pass, not a final style pass. The key success is that
the connected Beechwood compound remains visibly attached around its inner yard
instead of dissolving into detached cottages, which was Cycle U's main failure.
The remaining gap is style/camera: it still reads more like a high controlled
map plate than the original illustrated parish notebook scene.

## Strict Audit

- Attached/connected compound footprint: pass. The main compound stays
  connected around the courtyard, with the surrounding road, wall, garden, and
  tree relationships still recognizable from the tight control crop.
- Door/threshold audit: partial pass. The main compound has readable dark
  doorways/thresholds on the road-facing, yard-facing, and courtyard-facing
  facades. The small lower-left outbuilding has a readable dark doorway. The
  small lower-right outbuilding's entrance is weak/ambiguous; it should not be
  counted as a clean pass under the "every visible playable facade needs a
  readable door" rule.
- Camera/scale: partial pass. The plate is closer and more usable than earlier
  wide survey views, but the pitch is still high and roof/garden geometry still
  dominates more than the original notebook sample.
- Style vs original notebook: partial pass. Ink and watercolor language are
  present, and thatch/no-chimney behavior is useful. However, the garden plots
  and stone walls are too regular, the roof lines are too controlled, and the
  overall image lacks the sample's rougher low-camera density and muddy,
  lived-in surface variation.
- Semantic leaks: pass. No UI, labels, text, people, animals, carts, churches,
  chapels, graveyards, bridges, water, smoke, or chimney leakage is visible.

## Next Step

Use this image as a structure-preserving target for a style/camera refinement:
keep the connected compound and road/wall/garden relationships unchanged, lower
the camera feel, roughen the hand-watercolor surface, reduce sterile garden-grid
regularity without changing plot boundaries, and repair the small lower-right
outbuilding with a readable doorway/threshold.
