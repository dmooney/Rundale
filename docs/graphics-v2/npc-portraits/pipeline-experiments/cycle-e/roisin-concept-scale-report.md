# Cycle E Roisin Concept-Scale Check

Goal: make the portrait asset match the actual scale of the left-side notebook
concept portraits.

## Finding

The concept portrait crop is approximately `72 x 82` pixels at native concept
scale. Full-size generated masters are not the deliverable; they are source
material for a tiny UI crop.

## Artifacts

- Raw generated source:
  `docs/graphics-v2/npc-portraits/pipeline-experiments/cycle-e/npc-0004-roisin-connolly/e1.png`
- Concept-scale derivative:
  `docs/graphics-v2/npc-portraits/pipeline-experiments/cycle-e/npc-0004-roisin-connolly/e1-portrait-72x82.png`
- Comparison plate:
  `docs/graphics-v2/npc-portraits/pipeline-experiments/cycle-e/roisin-concept-scale-comparison.png`

## Read

At `72 x 82`, Cycle E is much closer to the concept than the full-size view
suggests. It is paper-backed, small, and readable. The remaining difference is
that the generated face/hair/clothing are still too resolved compared with the
concept's rougher marginal shorthand.

## Pipeline Change

For this style, judge candidates only after producing a `72 x 82` derivative.
Future prompts should continue to ask for native tiny notebook icons, but the
approval artifact is the concept-scale derivative, not the generation master.
