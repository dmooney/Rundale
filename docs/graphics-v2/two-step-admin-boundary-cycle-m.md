# Two-Step Admin-Boundary Experiment - Cycle M

Cycle M extends the Cycle L two-step pipeline with two changes:

1. The map-reader and top-down prompts explicitly classify unsupported dotted,
   pecked, dashed, or dot-chain lines as possible non-physical
   administrative/survey boundaries.
2. Grove uses a slightly wider source crop so the later isomorphic tilt has
   more north/top context from real map content.

The goal is to keep the accuracy gains from the top-down control stage while
preventing administrative map boundaries from becoming fake hedges, walls,
roads, ditches, or tree rows.

## Inputs

Both sites used the same generic procedure:

1. Original historic map crop.
2. Clean-context map-reader note generated with the same rubric.
3. Cleaned style swatches:
   - `style-crops/illustrated-style-field-wall-no-animals.png`
   - `style-crops/illustrated-style-wall-roof-no-props.png`

Grove used `pipeline-experiments/map-crop-grove-wide-admin-boundary-test.png`,
a wider crop made from the original attachment. Beechwood used
`map-sources/beechwood-map-crop-control-02.png`, which was already the full
control attachment.

The M2 isomorphic pass additionally received the M1 cleaned top-down plate for
the same crop.

## Outputs

| Site      | Map-reader note                                                            | M1 top-down control                                                | M2 isomorphic output                                                   | Result                                             |
| --------- | -------------------------------------------------------------------------- | ------------------------------------------------------------------ | ---------------------------------------------------------------------- | -------------------------------------------------- |
| Grove     | `pipeline-experiments/idea-m-grove-wide-map-reader-notes.md`               | `pipeline-experiments/idea-m-grove-wide-admin-topdown-cleaned.png` | `pipeline-experiments/idea-m-grove-wide-admin-two-step-isomorphic.png` | Strong topology; final near-16:9                   |
| Beechwood | `pipeline-experiments/idea-m-beechwood-admin-boundary-map-reader-notes.md` | `pipeline-experiments/idea-m-beechwood-admin-topdown-cleaned.png`  | `pipeline-experiments/idea-m-beechwood-admin-two-step-isomorphic.png`  | Strong admin-boundary suppression; final near-16:9 |

Each generated image has a `.prompt.md` sidecar and `.report.md` QA note in
`pipeline-experiments/`.

## What Changed

The new generic map-reader rule says unsupported dotted/pecked/dashed/dot-chain
linework may represent non-physical townland, parish, barony, county, estate,
parcel, or survey boundaries. If the map does not provide independent physical
evidence, the reader should classify that linework as administrative, survey,
non-physical, or ambiguous.

The M1 and M2 render prompts then say those lines must disappear into the field
texture. They must not be drawn as bushes, hedges, walls, fences, ditches,
paths, roads, crop rows, ridges, tree rows, shadows, or decorative texture.

Corroborating physical evidence still matters. A dotted/pecked line can be
rendered as physical only when supported by tree/hedge symbols on the line,
paired road edges, wall or ditch hatching, enclosure continuity, gate/yard
relationships, or another physical map mark.

## Verdict

Cycle M is the current best two-step candidate.

What worked:

- Beechwood's prominent eastern dotted administrative arc no longer appears as
  a continuous hedge, wall, or row of bushes.
- Grove's wider crop gave the top-down and final passes more real northern
  source content without pulling the distant church into the target plate.
- The M2 final plates were saved exactly as returned, with no post-generation
  edge extension, mirroring, padding, or synthetic margin fill.
- Both final plates kept the central route/enclosure/building topology readable
  and walkable.

What still needs work:

- The built-in image generator returned near-16:9 images (`1672x941`) rather
  than mathematically exact 16:9 outputs.
- Grove M1 returned `1548x1016`, so the first top-down stage still does not
  reliably honor native wide framing.
- Many physical boundaries are rendered as tidy stone walls. This is coherent
  visually, but future QA should check whether each wall is supported by the
  control plate and not by ambiguous survey linework.
- Outbuildings can become more finished and house-like than the map evidence
  justifies.

## Current Recommendation

Use Cycle M as the leading research path:

```text
map crop + generic legend/rubric
  -> clean-context map-reader note with admin-boundary ignore class
  -> top-down cleaned control plate
  -> isomorphic background plate
```

For the next test, keep the administrative-boundary rule and try to make the
M1 top-down stage produce exact native 16:9 without post-processing. If that
still fails, exact 16:9 may need to be solved by choosing source crops and game
plate dimensions outside the image model rather than by prompt language alone.
