# Tight Low-Camera Thatched Plates - Cycles T/U

Cycles T and U test a tighter crop strategy after Cycle R/S showed that scale
and camera were still the main gap from the original notebook sample.

Cycle T uses a cleaned low-camera building crop as a style/camera reference.
Cycle U adds corrected doorway style crops and a cleaned single-building
thatched/no-chimney reference.

## Inputs

The useful reference stack is:

1. Cycle R or T output for close-crop continuity.
2. Cycle M cleaned top-down control as topology authority.
3. Original historic map crop as source evidence.
4. `style-crops/illustrated-style-low-camera-slate-single-house-door-clean.png`
   for slate-roof facade, door, threshold, limewash, and low-camera cues.
5. `style-crops/illustrated-style-low-camera-thatched-single-house-door-clean.png`
   for thatch, no-chimney, door, threshold, and low-camera cues.
6. `illustrated-parish-notebook.png` only as broad style/mood reference.
7. The existing oblique warp as pitch cue.
8. Cleaned material swatches.

Do not use `style-crops/illustrated-style-low-camera-building-door-clean.png` or
`style-crops/illustrated-style-low-camera-thatched-door-clean.png` as reusable
references. Both have a good main house but retain partial foreground or
background building fragments that can teach the model that visible doorless
houses are acceptable.

## Outputs

| Site | Output | Prompt | Report | Result |
| --- | --- | --- | --- | --- |
| Grove T | `pipeline-experiments/idea-t-grove-tight-low-camera-clean-style-crop.png` | `pipeline-experiments/idea-t-grove-tight-low-camera-clean-style-crop.prompt.md` | `pipeline-experiments/idea-t-grove-tight-low-camera-clean-style-crop.report.md` | Strong tight-crop camera/scale pass |
| Grove U | `pipeline-experiments/idea-u-grove-tight-thatched-door-clean-style.png` | `pipeline-experiments/idea-u-grove-tight-thatched-door-clean-style.prompt.md` | `pipeline-experiments/idea-u-grove-tight-thatched-door-clean-style.report.md` | Best Grove art/material pass so far |
| Beechwood U | `pipeline-experiments/idea-u-beechwood-tight-thatched-door-clean-style.png` | `pipeline-experiments/idea-u-beechwood-tight-thatched-door-clean-style.prompt.md` | `pipeline-experiments/idea-u-beechwood-tight-thatched-door-clean-style.report.md` | Style/scale pass, topology caveat |

All returned `1672 x 941` PNGs.

## What Worked

- Tight crops are the strongest route so far for recovering notebook-style
  human scale.
- The cleaned low-camera style crops improve door/threshold readability.
- The single-house thatch reference successfully teaches rough thatch without
  chimneys or smoke.
- Grove U has the best combination so far of readable doors, larger facades,
  muddy yard staging, no-chimney thatch, and hand-drawn watercolor texture.
- Beechwood U shows that the style/material/camera direction can generalize
  beyond Grove without church, bridge, river, shop, UI, people, animal, cart,
  smoke, label, or chimney leakage.

## What Failed Or Remains Risky

- Beechwood U loosens the Cycle M/R/S connected courtyard footprint into a more
  separated farmstead cluster. That is good mood art, but not topology-clean
  enough for a production map-to-plate pipeline.
- Very tight crops can hide topology errors by omitting the wider structure.
- Stone walls and garden rows remain more regular than the original notebook
  sample.
- Roof area is still prominent; the camera is lower than Cycle R/S but not as
  low as the isolated style crops imply.

## Current Recommendation

Use the Cycle U art/camera/style direction, but pair it with stronger structure
control before batch production:

```text
historic map crop
  -> reproducible map-reader note
  -> tighter source/control crop chosen for desired sprite scale
  -> Cycle M-style topology control for that crop
  -> low-camera render using door-clean + single-house-thatch style crops
  -> topology audit against the control crop before accepting
```

For complex building footprints like Beechwood, do not rely on prompt-only
cropping from a wider generated plate. Generate or derive a tighter top-down
control crop first so the model cannot dissolve a connected courtyard into
separate buildings.
