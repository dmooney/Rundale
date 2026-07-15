# Tight Two-Step Wall/Door Cycle AT/AU

## Purpose

Cycle AT returns to the earlier two-step topology path after AR showed that a
tighter crop and stronger prompt text still let the model regularize roads into
a scenic centered Y/crossroads. The question was whether a top-down cleaned
control plate, generated from the tight playable crop, would recover the
map-derived road/building/garden topology while keeping AQ/AR's no-chimney roof
discipline and the original illustrated parish notebook style.

Cycle AU is a separate bounded visual repair of AT2. It should not be counted
as one-shot or direct recipe proof.

## Inputs

AT used only generic, repeatable inputs:

- Tight original map crop:
  `pipeline-experiments/idea-ar-kilteevan-playable-map-crop.png`
- Tight cleaned no-admin crop:
  `pipeline-experiments/idea-ar-kilteevan-playable-no-admin-map-crop.png`
- Tight oblique camera cue:
  `pipeline-experiments/idea-ar-kilteevan-playable-control-oblique-raw-warp.png`
- Full illustrated notebook sample, style only:
  `illustrated-parish-notebook.png`
- Clean single-building slate and thatch references:
  `style-crops/illustrated-style-low-camera-slate-single-house-door-clean.png`
  and
  `style-crops/illustrated-style-low-camera-thatched-single-house-door-clean.png`
- Tree/field watercolor reference:
  `style-crops/illustrated-style-trees-fields.png`

No hand-authored location-specific road, building, boundary, or landmark notes
were used.

## Outputs

| Cycle | Image                                                                  | Prompt                                                                       | Report                                                                       | Result                                            |
| ----- | ---------------------------------------------------------------------- | ---------------------------------------------------------------------------- | ---------------------------------------------------------------------------- | ------------------------------------------------- |
| AT1   | `pipeline-experiments/idea-at-kilteevan-tight-topdown-cleaned.png`     | `pipeline-experiments/idea-at-kilteevan-tight-topdown-cleaned.prompt.md`     | `pipeline-experiments/idea-at-kilteevan-tight-topdown-cleaned.report.md`     | Clean top-down plate; admin seam risk             |
| AT2   | `pipeline-experiments/idea-at-kilteevan-tight-two-step-isomorphic.png` | `pipeline-experiments/idea-at-kilteevan-tight-two-step-isomorphic.prompt.md` | `pipeline-experiments/idea-at-kilteevan-tight-two-step-isomorphic.report.md` | Strong camera/doors/no-chimneys; over-walled      |
| AU    | `pipeline-experiments/idea-au-kilteevan-at2-wall-door-repair.png`      | `pipeline-experiments/idea-au-kilteevan-at2-wall-door-repair.prompt.md`      | `pipeline-experiments/idea-au-kilteevan-at2-wall-door-repair.report.md`      | Best visual target from this crop; bounded repair |

## Result

AT1 produced a readable top-down watercolor control with broad roads, building
footprints, garden/orchard blocks, and no roof protrusions. Its failure is
important: the cleaned no-admin crop's erased diagonal seam still became a
real-looking hedge/field line in the generated control. A generated top-down
control can therefore smuggle cleaned-map artifacts into the next stage unless
the final prompt treats it as fallible and lets the raw/cleaned map veto it.

AT2 lifted AT1 into a stronger low 3/4 isomorphic game plate. It improved over
AR in the ways that matter for the notebook target:

- real facades, thresholds, and readable main-building doors,
- no visible chimneys, smoke, or roof nubs,
- coherent muddy road continuity,
- no UI, people, animals, water, church, shop, or label leakage,
- closer hand-inked watercolor texture than the direct-control render.

AT2 still is not the recipe endpoint. It inherited and amplified the wall
problem: garden edges, road shoulders, and some open-field divisions became too
clean and continuous. The composition is more controlled than AR, but it still
looks like a polished walled estate/farm map rather than the rougher original
parish notebook sample.

AU performs a bounded repair on AT2. It keeps the AT2 layout/camera and improves
the visual target:

- open fields breathe more,
- roads read less continuously walled,
- the garden boundary becomes rougher and less fortress-like,
- the right-side erased/admin seam no longer reads as a continuous physical
  feature,
- foreground and shed doors are readable doors with thresholds, not vague dark
  stains.

AU is useful as a visual target sample. It is not direct recipe evidence because
it uses AT2 as an edit target.

## Lessons

- The two-step path is still the best direction for combining topology and
  notebook style, but the generated top-down control must be treated as a
  fallible abstraction, not source truth.
- Do not ask the top-down stage to draw attractive stone or hedge boundaries
  around every enclosure. The final model tends to preserve and strengthen
  those outlines.
- The final-stage prompt should say continuous boundary marks from the top-down
  control are symbolic unless the raw/cleaned map supports them as domestic or
  garden compound edges.
- Door audits must inspect every enterable building at crop scale. AT2 passed
  the main house but left small shed doors marginal; AU fixed those.
- Keep AU in the "visual repair" bucket. The next recipe test should generate a
  fresh final plate from the same map/control evidence with weaker top-down
  wall authority and stronger "open fields first" language, rather than using
  AU as an edit target.

## Next Direction

For the next clean recipe attempt, keep AT's two-step structure but change the
authority model:

```text
map crop + cleaned no-admin crop
  -> top-down cleaned control with mostly terrain zones, roads, building
     footprints, planting, and very minimal symbolic boundaries
  -> low 3/4 isomorphic final where original/cleaned map veto generated
     boundary artifacts
  -> only if needed, bounded repair for concrete door/roof/wall failures
```

The key prompt change is to demote generated top-down stone/wall lines from
"physical boundaries" to "possible symbolic enclosure cues" unless corroborated
by the raw map. This should reduce the need for AU-style wall repair.
