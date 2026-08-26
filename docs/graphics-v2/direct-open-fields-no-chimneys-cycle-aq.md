# Direct Open Fields No Chimneys Cycle AQ

## Purpose

Cycle AQ tests whether the AO direct-control recipe can absorb the AP3 lessons
without using AP3 as an edit target: stronger conflict rules, stricter
single-building style-reference authority, and absolute no-chimney roof
language.

This is a fresh direct-control experiment. No previous generated plate is used
as an edit target or layout reference.

## Inputs

- Original map crop:
  `map-sources/kilteevan-z17-map-crop.png`
- Cleaned no-admin control:
  `pipeline-experiments/idea-aj2-kilteevan-dot-suppressed-no-admin-map-crop.png`
- Oblique camera cue:
  `pipeline-experiments/idea-aj2-kilteevan-dot-suppressed-no-admin-oblique-raw-warp.png`
- Full illustrated notebook sample, style only:
  `illustrated-parish-notebook.png`
- Clean single-building slate and thatch references:
  `style-crops/illustrated-style-low-camera-slate-single-house-door-clean.png`
  and
  `style-crops/illustrated-style-low-camera-thatched-single-house-door-clean.png`
- Tree/field watercolor reference:
  `style-crops/illustrated-style-trees-fields.png`

## Output

- `pipeline-experiments/idea-aq-kilteevan-direct-open-fields-no-chimneys.png`
- `pipeline-experiments/idea-aq-kilteevan-direct-open-fields-no-chimneys.prompt.md`
- `pipeline-experiments/idea-aq-kilteevan-direct-open-fields-no-chimneys.report.md`

## Result

AQ is visually strong but not a better direct-control recipe than AO/AP3.

What improved:

- notebook-style watercolor, ink, muddy road, and facade feel are stronger than
  AO/AP3,
- no obvious roof chimneys, roof stacks, smoke, or roof-mounted protrusions are
  visible,
- doors mostly remain readable at full-plate scale,
- no obvious church, bridge, water, UI, people, animals, labels, or smoke leaked
  from the full notebook reference.

What regressed:

- the image reads as a composed picturesque rural crossroads rather than a close
  transformation of the supplied map/control crop,
- it appears to add or regularize building groups beyond the dark roof marks,
- walls and gatepost-like boundary fragments are more assertive than AO,
- open-field restraint is weaker than AO/AP3, even though it does not restore a
  clear deleted admin-boundary trace.

## Interpretation

AQ proves that stronger conflict rules and absolute no-chimney roof language can
help a fresh generation avoid the roof-stack failure. It also shows that the
model may compensate by falling back to a prettier generic crossroads scene,
with more walling and more compositional freedom.

Do not promote AQ over AO/AP3 as the current pipeline direction. Treat it as:

- positive evidence for no-chimney prompt language,
- positive evidence for notebook style pressure,
- negative evidence for leaving composition and building grouping too free in a
  fresh direct-control pass.

AO remains the better direct-control evidence for open-field topology. AP3
remains the better cleaned AO visual target, with its whole-plate repaint
caveat.

## Next Direction

The next production-shaped attempt should keep AQ's roof/conflict wording but
reduce composition freedom:

- derive a tighter source/control crop for the desired playable plate scale,
- preserve the exact relative building/road grouping from the control crop more
  explicitly,
- discourage "picturesque crossroads" composition,
- keep ordinary field boundaries even softer than AO,
- continue using only approved single-building style crops for doors/roofs.
