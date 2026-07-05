# Cycle D Roisin Style Calibration

Goal: match the tiny left-side people-list head sketches in
`illustrated-parish-notebook.png`.

## Artifacts

- Reference rail crop:
  `docs/graphics-v2/npc-portraits/references/illustrated-notebook-left-people-list.png`
- Tight no-label-ish reference crop:
  `docs/graphics-v2/npc-portraits/references/illustrated-notebook-head-style-tight.png`
- Rejected Cycle A:
  `docs/graphics-v2/npc-portraits/pipeline-experiments/cycle-a/npc-0004-roisin-connolly/a1.png`
- Cycle B:
  `docs/graphics-v2/npc-portraits/pipeline-experiments/cycle-b/npc-0004-roisin-connolly/b1.png`
- Cycle C:
  `docs/graphics-v2/npc-portraits/pipeline-experiments/cycle-c/npc-0004-roisin-connolly/c1.png`
- Preferred current calibration:
  `docs/graphics-v2/npc-portraits/pipeline-experiments/cycle-d/npc-0004-roisin-connolly/d1.png`
- UI derivatives:
  `docs/graphics-v2/npc-portraits/pipeline-experiments/cycle-d/npc-0004-roisin-connolly/d1-thumb-96.png`
  and
  `docs/graphics-v2/npc-portraits/pipeline-experiments/cycle-d/npc-0004-roisin-connolly/d1-thumb-64.png`
- Comparison plate:
  `docs/graphics-v2/npc-portraits/pipeline-experiments/cycle-d/roisin-style-comparison.png`

## Read

- **Cycle A failed.** It became a polished watercolor bust portrait, not a
  notebook-margin people-list icon.
- **Cycle B improved medium.** It removed color and card treatment but was still
  a formal portrait sketch.
- **Cycle C improved scale.** It added paper space and better UI-icon sizing,
  but the drawing remained too dense/formal.
- **Cycle D is the best current calibration.** It is still slightly more
  anatomically polished than the concept reference, but at 64-96 px it reads
  close enough to test another NPC.

## Prompt Rule Change

The reusable prompt must lead with "tiny rough 64 px marginal icon" and a strict
line/detail budget. If the prompt starts with "portrait," "head sketch," or
"watercolor," the model over-renders.
