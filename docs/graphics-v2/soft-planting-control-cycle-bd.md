# Soft Planting Control Cycle BD

## Purpose

Cycle BD tests a stronger version of BC's boundary-material idea. BC tinted
garden/orchard texture green, but it still left crisp perimeter linework for
the image model to turn into walls. BD adds a more destructive deterministic
soft-planting control that suppresses wall-like planting edges before the final
fresh render.

This is a fresh render test. The final image did not use BA, BB, AZ, or any
other prior isomorphic plate as an input reference.

## Script Change

`scripts/prototype_map_controls.py` now also emits:

- `*-soft-planting-control.png`
- `*-soft-planting-oblique.png`

The soft-planting control is deterministic and pixel-derived. It:

- fills likely garden/orchard/scrub texture as muted planting material,
- separates a soft planting core from a suppressed, low-contrast edge,
- suppresses source linework inside planting so perimeter outlines are less
  wall-like,
- keeps any road/yard cue extremely weak and outside planting,
- preserves suppressed/no-data comparison areas as a muted veto cue.

It does not hand-author roads, buildings, walls, or per-location
interpretations.

## Inputs

BD used:

- Tight original map crop:
  `pipeline-experiments/idea-ar-kilteevan-playable-map-crop.png`
- Tight cleaned no-admin crop:
  `pipeline-experiments/idea-ar-kilteevan-playable-no-admin-map-crop.png`
- Top-down cleaned control:
  `pipeline-experiments/idea-at-kilteevan-tight-topdown-cleaned.png`
- New soft-planting control:
  `pipeline-experiments/idea-bd-kilteevan-soft-planting-soft-planting-control.png`
- Oblique camera cue:
  `pipeline-experiments/idea-ar-kilteevan-playable-control-oblique-raw-warp.png`
- Full notebook UI sample, style only:
  `illustrated-parish-notebook.png`
- Clean single-building slate and thatch references:
  `style-crops/illustrated-style-low-camera-slate-single-house-door-clean.png`
  and
  `style-crops/illustrated-style-low-camera-thatched-single-house-door-clean.png`
- Tree/field watercolor reference:
  `style-crops/illustrated-style-trees-fields.png`

No hand-authored location-specific notes were used.

## Prompt Change

The BD prompt keeps the BC roof and boundary rules, then strengthens the door
audit rule from "readable doorway" to "doors on openings":

- every visible walkable building that shows a facade needs a readable timber
  plank door plus threshold,
- if the model paints any person-sized dark vertical opening, doorway, shed
  mouth, barn mouth, or black rectangular hole, it must place a visible timber
  plank door directly inside that opening,
- empty black door holes, shadow-only doorways, and doors placed beside an
  unfilled opening are failures.

This wording was added after foreground-building audits showed that vague door
language can pass the main house while leaving smaller or foreground buildings
doorless.

## Outputs

| Artifact              | Path                                                                             |
| --------------------- | -------------------------------------------------------------------------------- |
| Control report        | `pipeline-experiments/idea-bd-kilteevan-soft-planting-control-report.md`         |
| Soft-planting control | `pipeline-experiments/idea-bd-kilteevan-soft-planting-soft-planting-control.png` |
| Soft-planting oblique | `pipeline-experiments/idea-bd-kilteevan-soft-planting-soft-planting-oblique.png` |
| Fresh render          | `pipeline-experiments/idea-bd-kilteevan-soft-planting-fresh-notebook.png`        |
| Fresh prompt          | `pipeline-experiments/idea-bd-kilteevan-soft-planting-fresh-notebook.prompt.md`  |
| Fresh report          | `pipeline-experiments/idea-bd-kilteevan-soft-planting-fresh-notebook.report.md`  |

## Result

BD is a useful partial improvement, not a clean recipe pass.

What improved:

- The roof rule is much better than BC: no obvious chimneys, smoke, stacks,
  vents, or roof nubs.
- Close-up audits of the upper houses, the foreground house, and the small
  foreground sheds show plank-door faces fitted into visible person-sized
  openings rather than empty black holes.
- The plate keeps a strong notebook watercolor/ink look and readable facades.
- No UI, people, animals, church, river, bridge, shop, text, or smoke leakage is
  apparent.

What failed:

- The road network still regularizes into a scenic central junction more than
  the map crop warrants.
- Garden internals still acquire path-like tan lines.
- Several garden/perimeter lines become hard fence, wall, or trellis language
  despite the soft-planting control.
- A lower-right diagonal vegetation trace may still preserve suppressed
  admin/no-data linework as a physical hedge/tree chain.

## Interpretation

The explicit "doors on openings" instruction is worth keeping. It appears to
fix the foreground/shed door failure mode without hurting roofs.

The soft-planting control is directionally right but not strong enough by
itself. It removed some crisp perimeter authority, yet the model still
preferred composed garden boundaries and a scenic road junction. The next
recipe needs either stronger source-layout locking or a control that says less,
not more, about internal garden geometry.

## Current Recommendation

Do not promote BD as the final one-shot recipe.

- `BA` remains the best fresh notebook recipe attempt before the boundary
  experiments.
- `BB` remains the best softened edit from BA.
- `BD` is the best fresh roof/door discipline result in this branch, but its
  road/garden topology is not clean enough.
- Keep BD's "doors on openings" language in future prompts.
