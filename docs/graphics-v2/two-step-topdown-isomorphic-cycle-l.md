# Two-Step Top-Down To Isomorphic Experiment - Cycle L

Cycle L tests whether image generation should be split into two raster stages:

1. Generate a cleaned, top-down illustrated control plate from the original map
   crop and reproducible map-reader notes.
2. Convert that cleaned top-down plate into a north-up 3/4 orthographic
   isomorphic game background, while still cross-checking the original map crop
   and map-reader notes.

## Inputs

Both Grove and Beechwood used the same procedure:

1. Original historic map crop.
2. Existing Cycle K map-reader note for that crop.
3. Cleaned style swatches:
   - `style-crops/illustrated-style-field-wall-no-animals.png`
   - `style-crops/illustrated-style-wall-roof-no-props.png`

The L2 isomorphic pass additionally received the L1 cleaned top-down plate for
the same crop.

## Outputs

| Site      | L1 top-down control                                         | L2 isomorphic output                                            | Result                                               |
| --------- | ----------------------------------------------------------- | --------------------------------------------------------------- | ---------------------------------------------------- |
| Grove     | `pipeline-experiments/idea-l-grove-topdown-cleaned.png`     | `pipeline-experiments/idea-l-grove-two-step-isomorphic.png`     | Useful, especially for topology                      |
| Beechwood | `pipeline-experiments/idea-l-beechwood-topdown-cleaned.png` | `pipeline-experiments/idea-l-beechwood-two-step-isomorphic.png` | Useful control, final plate has edge-artifact caveat |

Each output has a `.prompt.md` sidecar and `.report.md` QA note in
`pipeline-experiments/`.

## Verdict

Cycle L appears more accurate than Cycle K for central source topology and map
geometry. It should be treated as a promising candidate path, with the remaining
open issue being native 16:9 framing/edge discipline rather than the core
two-step method.

What worked:

- L1 top-down controls were strong on both sites. They removed map labels/noise
  while keeping roads, enclosures, building footprints, gardens, woodland, and
  field boundaries readable.
- Grove L2 produced a coherent final isomorphic scene with good topology and
  clean building separation.
- Beechwood L2 preserved the central road, dominant building range, garden
  enclosure, woodland mass, and negative evidence against church/water.

What still needs work:

- Beechwood's L2 output needed horizontal edge extension to become 16:9, and
  the extended margins should not be treated as source topology.
- The two-step process can inherit and amplify cleaned-control artifacts. The
  top-down plate is useful evidence, but it is not a substitute for checking the
  original map and map-reader note.
- The L2 results are a little more control-plate-like than Cycle K in places,
  but the central map read appears stronger.

## Current Recommendation

Keep Cycle K as the simpler one-step baseline:

```text
map crop -> reproducible map-reader note -> one-step isomorphic render
```

Treat Cycle L as the promising accuracy path:

```text
map crop + map-reader note -> top-down cleaned control plate
top-down cleaned control plate + map crop + note -> isomorphic render
```

The next useful test is not whether the two-step idea works; it does. The next
test is whether a stricter native-16:9 L1/L2 prompt can avoid edge extension
while preserving the improved map fidelity on both Grove and Beechwood.
