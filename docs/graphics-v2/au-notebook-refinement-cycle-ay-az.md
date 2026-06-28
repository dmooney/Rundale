# AU Notebook Refinement Cycle AY/AZ

## Purpose

Cycle AY/AZ tests whether the best current topology-preserving visual target for
the tight Kilteevan crop, Cycle AU, can be pushed closer to the original
illustrated parish notebook look without losing the Cycle AT/AU map-derived
layout discipline.

This is deliberately a bounded visual-refinement branch. It is not one-shot
recipe evidence because both AY and AZ use previous rendered plates as edit
targets.

## Inputs

AY used:

- Edit target:
  `pipeline-experiments/idea-au-kilteevan-at2-wall-door-repair.png`
- Full notebook UI sample, style only:
  `illustrated-parish-notebook.png`
- Clean single-building slate and thatch references:
  `style-crops/illustrated-style-low-camera-slate-single-house-door-clean.png`
  and
  `style-crops/illustrated-style-low-camera-thatched-single-house-door-clean.png`
- Tree/field watercolor reference:
  `style-crops/illustrated-style-trees-fields.png`

AZ used AY as the edit target with the same style references. No hand-authored
location-specific road, building, boundary, or landmark notes were used.

## Outputs

| Cycle | Image | Prompt | Report | Result |
| --- | --- | --- | --- | --- |
| AY | `pipeline-experiments/idea-ay-kilteevan-au-notebook-style-refine.png` | `pipeline-experiments/idea-ay-kilteevan-au-notebook-style-refine.prompt.md` | `pipeline-experiments/idea-ay-kilteevan-au-notebook-style-refine.report.md` | Better notebook texture and doors, topology broadly preserved |
| AZ | `pipeline-experiments/idea-az-kilteevan-ay-low-camera-refine.png` | `pipeline-experiments/idea-az-kilteevan-ay-low-camera-refine.prompt.md` | `pipeline-experiments/idea-az-kilteevan-ay-low-camera-refine.report.md` | Strongest visual target from this Kilteevan branch, still not recipe proof |

## Result

AY improved AU in the right visual direction:

- richer sepia ink and watercolor variation,
- rougher muddy roads and vegetation,
- clearer plank doors and thresholds on visible buildings,
- no obvious chimneys, smoke, people, animals, UI, church, river, bridge, or
  shop leakage,
- no AX-style drift into a new picturesque crossroads.

AZ is the stronger of the two visual refinements. It keeps the main AU/AY
relationships: main house, three lower sheds, upper cottage cluster, garden
block, road junctions, open fields, scrub/tree masses, fences, gates, and yards.
It improves facade weight and door readability, makes several buildings feel
less like flat roof symbols, and pushes the linework closer to the notebook
sample.

The caveat is still important: AZ darkens and sharpens some garden fencing and
garden-plot structure. It does not create an AX-style wall network, but it shows
that even bounded style/camera edits can spend boundary-restraint budget. Treat
it as a visual target, not as proof that the reusable recipe is solved.

## Door Prompt Lesson

The "doors on the openings" wording was materially better than asking for
"readable doorways." The useful rule is:

```text
Every person-sized dark vertical opening on any visible building must contain a
visible wooden plank door. A door means an actual brown or weathered gray-brown
timber slab or half-open plank door with vertical plank marks, not a black hole
or vague shadow.
```

Keep that language in future final-render and repair prompts, and audit every
visible walkable building, including sheds and partial edge buildings.

## Current Recommendation

- Use AZ as the best current visual target for this specific Kilteevan tight
  crop.
- Use AT/AU as the better topology-preserving recipe signal.
- Do not promote AY/AZ to one-shot proof: they are edit-target refinements.
- Keep AX only as a door-repair example for the topology-poor AW branch.

## Next Direction

The goal remains to get AZ-like style from a cleaner recipe path:

```text
raw map crop + cleaned no-admin crop
  -> top-down or deterministic control with minimal boundary authority
  -> low 3/4 notebook render with explicit doors-on-openings rule
  -> bounded repair only for concrete audit failures
```

The unresolved reusable-pipeline problem is still geometry authority: how to
retain AU/AZ's topology restraint while getting the notebook richness without a
previous rendered plate.
