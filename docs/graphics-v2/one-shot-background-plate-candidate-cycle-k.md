# One-Shot Background Plate Candidate - Cycle K

Cycle K extends Cycle J with a reproducible map-reader stage. The same generic
map-reader rubric was run in fresh contexts for Grove and Beechwood, producing
confidence-graded notes from each map crop. The render stage then received the
map crop, the note file for that crop, and the same two cleaned style swatches.

This is the current best candidate for building interpretation.

## Input Order

Use this exact render input order:

1. Target historic map crop. This remains the primary layout/content reference.
2. `style-crops/illustrated-style-field-wall-no-animals.png`
3. `style-crops/illustrated-style-wall-roof-no-props.png`
4. Data-derived map-reader note generated from the same crop using
   `map-reader-stage-template.md`.

The note file is allowed to be location-specific because it is generated from
the local map crop. The procedure is not allowed to be location-specific.

## Validation Outputs

| Control crop | Map-reader notes | Output | Report | Result |
| --- | --- | --- | --- | --- |
| Grove | `pipeline-experiments/idea-k-grove-map-reader-notes.md` | `pipeline-experiments/idea-k-grove-map-reader-guided.png` | `pipeline-experiments/idea-k-grove-map-reader-guided.report.md` | Pass, better building interpretation |
| Beechwood | `pipeline-experiments/idea-k-beechwood-map-reader-notes.md` | `pipeline-experiments/idea-k-beechwood-map-reader-guided.png` | `pipeline-experiments/idea-k-beechwood-map-reader-guided.report.md` | Pass, better building interpretation |

## Result Summary

Compared with Cycle J, Cycle K produced stronger building groups while keeping
the important semantic guardrails:

- Grove kept the modest primary roadside building, smaller service buildings,
  planted enclosure, and the larger ambiguous ancillary/open-walled structure.
- Beechwood kept the dominant rectilinear/courtyard-adjacent range and smaller
  subordinate structures, rather than inventing a church, shop, watercourse, or
  bridge.
- Both renders kept north-up isometric framing, no UI/text, no copied style
  objects, no smoke, and no unsupported church/graveyard/water.

## Current Read

Cycle K should supersede Cycle J when the goal is usable production-style
background plates. Cycle J remains useful as a simpler baseline when no
map-reader note is available. The best current pipeline is:

```text
historic map crop
  -> clean-context reproducible map-reader note
  -> map crop + note + cleaned style swatches + generic render prompt
  -> illustrated north-up isometric background plate
```

The generated note is an auditable artifact. If the render looks wrong, inspect
whether the note over-read the map, under-described a footprint, or failed to
state negative evidence.
