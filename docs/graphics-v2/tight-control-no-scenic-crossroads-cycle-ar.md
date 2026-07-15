# Tight Control No Scenic Crossroads Cycle AR

## Purpose

Cycle AR tests the next step after AQ: keep AQ's stronger roof/conflict
language, but reduce composition freedom by giving the model a tighter playable
map/control crop instead of the wider Kilteevan source crop.

This is a fresh direct-control experiment. No previous generated plate is used
as an edit target, style target, layout reference, or composition reference.

## Inputs

Primary render inputs:

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

Additional crop/control artifacts created during setup:

- `pipeline-experiments/idea-ar-kilteevan-tight-map-crop.png`
- `pipeline-experiments/idea-ar-kilteevan-tight-no-admin-map-crop.png`
- `pipeline-experiments/idea-ar-kilteevan-tight-control-*`
- `pipeline-experiments/idea-ar-kilteevan-playable-control-*`

The prototype control script reported zero reliable building-like components on
both tight crops, so semantic masks and extruded blockouts are not content
authorities. Only the map crops and oblique raw warp are useful here.

## Output

- `pipeline-experiments/idea-ar-kilteevan-tight-control-no-scenic-crossroads.png`
- `pipeline-experiments/idea-ar-kilteevan-tight-control-no-scenic-crossroads.prompt.md`
- `pipeline-experiments/idea-ar-kilteevan-tight-control-no-scenic-crossroads.report.md`

## Result

AR is the best fresh direct-control direction after AQ, but still not a final
recipe.

What improved over AQ:

- tighter playable scale,
- fewer invented extra buildings,
- stronger no-chimney/no-roof-nub discipline,
- good notebook-style ink, watercolor, facades, roads, and fields,
- no obvious semantic leakage from the full notebook sample,
- broad open fields remain mostly soft.

What remains unsolved:

- the roads are still regularized into a centered scenic Y/crossroads,
- yard and road edges still get more wall/fence fragments than the ideal AO
  open-field restraint,
- the result is more composed than the awkward source crop,
- the tight crop helped, but prompt text alone still does not lock road geometry
  strongly enough.

## Interpretation

AR keeps AQ's best lesson, the roof/conflict language, while recovering some of
the control discipline that AQ lost. It is better one-shot recipe evidence than
AP3 because it is a fresh generation, not a repair. It is still weaker than
AO/AP3 as proof of faithful Kilteevan topology because the centered Y-road
composition is too scenic.

Do not mark the pipeline solved from AR. Use AR as evidence that the next
attempt needs a stronger road/topology control, not just a tighter crop and more
negative wording.

## Next Direction

The next direct-control pass should preserve AR's crop scale and no-chimney
language, but add a deterministic road/yard topology cue that makes the model
hold the exact road corridors and building-group offsets without converting
them into a neat centered crossroads.
