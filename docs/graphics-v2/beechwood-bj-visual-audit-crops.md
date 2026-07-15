# Beechwood BJ Visual Audit Crops

## Purpose

These crops make the remaining gap in BJ easier to see without relying on
whole-plate impressions. They compare the original notebook art target, the
Cycle Q/BJ Beechwood renders, and the Cycle M topology control in fixed
regions of the shared 1672x941 frame.

The crops are evidence for the next imagegen prompt, not production assets.

## Crop Set

- `pipeline-experiments/idea-bj-audit-original-style-scene.png` — original
  notebook art sample crop for rough ink, muddy road, facade, vegetation, and
  paper-grain feel only.
- `pipeline-experiments/idea-bj-audit-original-facade-road.png` — closer
  original notebook crop emphasizing roads and facades.
- `pipeline-experiments/idea-bj-audit-beechwood-core-q.png` — Cycle Q
  Beechwood core.
- `pipeline-experiments/idea-bj-audit-beechwood-core-bj.png` — Cycle BJ
  Beechwood core.
- `pipeline-experiments/idea-bj-audit-beechwood-core-m.png` — Cycle M topology
  core.
- `pipeline-experiments/idea-bj-audit-garden-regular-q.png` — Cycle Q garden
  crop.
- `pipeline-experiments/idea-bj-audit-garden-regular-bj.png` — Cycle BJ garden
  crop.
- `pipeline-experiments/idea-bj-audit-garden-regular-m.png` — Cycle M garden
  topology crop.
- `pipeline-experiments/idea-bj-audit-compound-facades-q.png` and
  `pipeline-experiments/idea-bj-audit-compound-facades-bj.png` — compound
  facade comparison.
- `pipeline-experiments/idea-bj-audit-lower-buildings-q.png` and
  `pipeline-experiments/idea-bj-audit-lower-buildings-bj.png` — lower-building
  door/topology comparison.

## Findings

BJ improves over Q in the places that matter most for playable buildings:
facades are heavier, doors are more literal, roof marks read as slate hatching
rather than protrusions, and the connected compound remains intact.

The unresolved problem is concentrated in the garden and enclosure texture:

- garden compartments still read as crisp plan-view rectangles,
- rows are still repeated with machine-like spacing,
- thin row marks risk reading as small wall caps or masonry lines,
- the garden perimeter is still too clean and continuous,
- open fields have more notebook texture than Q, but not the same muddy,
  scumbled irregularity as the original sample.

Cycle M's garden crop explains the trap: it is useful topology evidence but is
inherently diagrammatic. Future prompts should use M only to preserve the
garden footprint and broad internal organization, while using BJ/Q garden
crops to identify what needs softening.

## Prompt Implication

Prefer the queued Cycle BL prompt over the older BK prompt when imagegen credits
return. BL gives the model local crop evidence and states the failure more
concretely: soften garden rows without adding walls, preserve the connected
compound/facades/doors, and keep topology preservation above style pressure.
