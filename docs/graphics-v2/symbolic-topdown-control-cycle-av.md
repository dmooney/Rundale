# Symbolic Topdown Control Cycle AV

## Purpose

Cycle AV tests the next clean recipe after AT/AU. AT showed that a generated
top-down control can improve final camera, doors, and roof discipline, but it
also smuggled continuous wall and admin-seam artifacts into the isomorphic pass.

AV changes the top-down prompt: the control should be mostly terrain zones,
roads, building footprints, planting, and tree masses, with boundaries kept
sparse and symbolic rather than physical.

## Inputs

AV used the same generic, repeatable evidence stack as AT:

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

| Cycle | Image                                                                     | Prompt                                                                          | Report                                                                          | Result                                                |
| ----- | ------------------------------------------------------------------------- | ------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- | ----------------------------------------------------- |
| AV1   | `pipeline-experiments/idea-av-kilteevan-symbolic-topdown-control.png`     | `pipeline-experiments/idea-av-kilteevan-symbolic-topdown-control.prompt.md`     | `pipeline-experiments/idea-av-kilteevan-symbolic-topdown-control.report.md`     | Less hard walling, but wider/scenic and still bounded |
| AV2   | `pipeline-experiments/idea-av-kilteevan-symbolic-two-step-isomorphic.png` | `pipeline-experiments/idea-av-kilteevan-symbolic-two-step-isomorphic.prompt.md` | `pipeline-experiments/idea-av-kilteevan-symbolic-two-step-isomorphic.report.md` | Strong style, but not a recipe success                |

## Result

AV1 partially did what the prompt asked: it reduced the stone-wall feel compared
with AT1. It still failed the highest-priority control goal in two ways:

- it expanded/regularized the scene into a more scenic local plan than the tight
  crop warrants,
- it kept continuous compound/garden outlines plus faint diagonal/field seams
  that could still become physical boundaries downstream.

AV2 is visually attractive and keeps the no-chimney discipline, readable main
facades, muddy roads, and notebook watercolor style. It is not better than AU as
a target and not better than AT as recipe evidence:

- roads and yards remain walkable, but composition drifts toward a centered
  scenic crossroads,
- the final plate still contains continuous garden/compound/road-edge boundary
  chains,
- the small shed cluster is weaker on door/threshold readability than AU,
- several small structures appear enlarged or regularized beyond the tight map
  crop evidence.

Close-up audit found the main house has a usable doorway, but the small shed
cluster returns to mostly roof/wall texture with marginal or missing entrances.

## Interpretation

AV is a useful negative result. Weakening generated top-down boundary authority
can reduce some wall hardness, but it does not solve the underlying issue if the
generated control is still allowed to redraw the scene as a prettier plan.

The generated top-down stage is doing too much creative interpretation. It is
not merely cleaning the map; it is choosing composition, regularizing
buildings, and inventing enclosure confidence. Those choices then bias the final
render even when the final prompt says the raw map should veto them.

## Current Recommendation

Do not promote AV over AT/AU.

- Use AT as the stronger two-step recipe signal.
- Use AU as the current best visual target for this crop.
- Treat AV as evidence that "make the top-down control more symbolic" is not
  enough when the control is still generated freehand.

## Next Direction

The next credible recipe should reduce creative freedom before imagegen, not
just in the prompt:

```text
raw/cleaned map crop
  -> deterministic or very literal control image that preserves crop extent,
     road corridors, building marks, and planted regions without beautifying
     them
  -> final low 3/4 isomorphic render with raw/cleaned map veto
```

Candidate controls:

- a lightly stylized version of the cleaned map crop with typography muted but
  geometry intact,
- a flat color "paint by numbers" control with only roads, building marks,
  planted areas, tree masses, and open fields,
- a literal top-down repaint/edit constrained to preserve the exact crop and
  avoid drawing new enclosing lines.

If the top-down stage remains image-generated, it should be asked to preserve
the raw map's awkward crop and leave field/garden boundaries almost entirely
uninterpreted. Otherwise it will keep making plausible but overconfident wall
systems.
