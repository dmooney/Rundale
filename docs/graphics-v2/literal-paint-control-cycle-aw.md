# Literal Paint Control Cycle AW

## Purpose

Cycle AW tests the next step after AV: replace the generated top-down control
with a deterministic literal paint-by-numbers control. The goal was to preserve
the tight crop geometry and feature uncertainty without letting imagegen
beautify the plan before the final render.

## Script Change

`scripts/prototype_map_controls.py` now emits two additional artifacts:

- `*-literal-paint-control.png` — a 512x288 flat control using the cleaned crop
  geometry.
- `*-literal-paint-oblique.png` — the same literal control squashed into the
  low-camera pitch cue frame.

The new control is generic and pixel-derived:

- muted green: ordinary open field / grass context,
- soft tan: weak road/yard candidate hint,
- dark gray/brown: source map linework evidence, not automatically walls,
- green dots/blobs: likely tree/scrub symbols,
- muted gray-green: original-vs-cleaned differences, treated as suppressed
  admin/no-data zones.

The script still reports the known caveat: this crop produces zero reliable
building-like components under the current classifier. Building existence must
therefore remain a raw-map/imagegen inference, not a deterministic class.

## Inputs

- Tight original map crop:
  `pipeline-experiments/idea-ar-kilteevan-playable-map-crop.png`
- Tight cleaned no-admin crop:
  `pipeline-experiments/idea-ar-kilteevan-playable-no-admin-map-crop.png`
- Literal paint control:
  `pipeline-experiments/idea-aw2-kilteevan-literal-paint-literal-paint-control.png`
- Literal paint oblique cue:
  `pipeline-experiments/idea-aw2-kilteevan-literal-paint-literal-paint-oblique.png`
- Cleaned map oblique cue:
  `pipeline-experiments/idea-aw2-kilteevan-literal-paint-oblique-raw-warp.png`
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

| Artifact               | Path                                                                              |
| ---------------------- | --------------------------------------------------------------------------------- |
| Literal control report | `pipeline-experiments/idea-aw2-kilteevan-literal-paint-control-report.md`         |
| Literal paint control  | `pipeline-experiments/idea-aw2-kilteevan-literal-paint-literal-paint-control.png` |
| Literal paint oblique  | `pipeline-experiments/idea-aw2-kilteevan-literal-paint-literal-paint-oblique.png` |
| Final image            | `pipeline-experiments/idea-aw-kilteevan-literal-control-isomorphic.png`           |
| Final prompt           | `pipeline-experiments/idea-aw-kilteevan-literal-control-isomorphic.prompt.md`     |
| Final report           | `pipeline-experiments/idea-aw-kilteevan-literal-control-isomorphic.report.md`     |

## Result

AW is a strong visual/style sample but a bad recipe result.

What worked:

- excellent low 3/4 notebook-style facades,
- readable dark doors and thresholds on visible buildings,
- strong slate/thatch material handling,
- no visible chimneys, smoke, people, animals, water, church, shop, or UI,
- muddy road texture and hand-inked watercolor richness are close to the
  original parish notebook target.

What failed:

- the model regularized the crop into a picturesque centered crossroads,
- it invented or over-emphasized a tidy walled hamlet structure,
- continuous stone walls and road-border walls are much stronger than the map
  supports,
- map-derived topology is worse than AT/AU and worse than the literal control
  should allow.

Close-up audits confirmed the visual win: doors and roof discipline are good.
The failure is spatial/topological, not style.

## Interpretation

The literal control did not solve the final-render problem. The image model can
still treat the reference stack as permission to create a plausible rural
village plate instead of a strict map transformation. The issue is no longer
only "generated top-down controls compose creatively"; even deterministic class
controls can be overridden by the model's scenic prior.

This suggests the final-stage call is doing too much at once:

```text
infer map features + preserve topology + choose building forms
  + lower camera + apply notebook style + avoid walls/chimneys
```

The style/camera target is now well understood. The remaining problem is a
strong enough geometry constraint.

## Current Recommendation

Do not promote AW over AT/AU.

- Use AU as the current best visual target for this crop.
- Use AT as the better two-step recipe signal.
- Use AW as style/camera evidence for doors, thatch/slate, and no-chimney
  discipline, but not topology evidence.

## Next Direction

The next credible pipeline should make geometry less optional before the final
imagegen call:

1. Build a deterministic geometric blockout or orthographic warp with explicit
   road corridors, building footprints, planted zones, and no-data/admin zones.
2. Avoid freehand top-down imagegen controls for crop/layout.
3. Consider a bounded image edit/transform path from a control plate rather than
   an unconstrained fresh generation.
4. If using a fresh generation, reduce reference count and style pressure until
   topology passes, then do a separate bounded style pass.

The literal paint control is still useful as an input artifact, but by itself it
does not make the final image model respect the crop.
