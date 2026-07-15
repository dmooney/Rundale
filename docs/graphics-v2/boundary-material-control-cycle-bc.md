# Boundary Material Control Cycle BC

## Purpose

Cycle BC tests the next hypothesis after BA/BB: the final model needs a
reproducible control artifact that says "soft planting/material, not wall" for
garden/orchard/internal marks. The goal was to reduce BA's hard walled garden
without relying on a prior isomorphic edit target.

This is a fresh render test. The final image did not use BA, BB, AZ, or any
other prior isomorphic plate as an input reference.

## Script Change

`scripts/prototype_map_controls.py` now emits:

- `*-boundary-material-control.png`
- `*-boundary-material-oblique.png`

The boundary-material control is deterministic and pixel-derived. It:

- keeps the cleaned crop geometry,
- demotes ordinary linework to faint evidence,
- marks dense garden/orchard/scrub/tree texture as soft planting material,
- keeps road/yard hints pale and weak,
- marks original-vs-cleaned differences as muted no-data/admin-deletion zones.

It does not infer confident walls. That is intentional: wall authority should
be rare.

## Inputs

BC used:

- Tight original map crop:
  `pipeline-experiments/idea-ar-kilteevan-playable-map-crop.png`
- Tight cleaned no-admin crop:
  `pipeline-experiments/idea-ar-kilteevan-playable-no-admin-map-crop.png`
- Top-down cleaned control:
  `pipeline-experiments/idea-at-kilteevan-tight-topdown-cleaned.png`
- New boundary-material control:
  `pipeline-experiments/idea-bc-kilteevan-boundary-material-boundary-material-control.png`
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

## Outputs

| Artifact                  | Path                                                                                     |
| ------------------------- | ---------------------------------------------------------------------------------------- |
| Control report            | `pipeline-experiments/idea-bc-kilteevan-boundary-material-control-report.md`             |
| Boundary-material control | `pipeline-experiments/idea-bc-kilteevan-boundary-material-boundary-material-control.png` |
| Boundary-material oblique | `pipeline-experiments/idea-bc-kilteevan-boundary-material-boundary-material-oblique.png` |
| Fresh render              | `pipeline-experiments/idea-bc-kilteevan-boundary-material-fresh-notebook.png`            |
| Fresh prompt              | `pipeline-experiments/idea-bc-kilteevan-boundary-material-fresh-notebook.prompt.md`      |
| Fresh report              | `pipeline-experiments/idea-bc-kilteevan-boundary-material-fresh-notebook.report.md`      |

## Result

BC is a useful negative result.

What improved:

- It remains a fresh no-prior-render plate.
- Broad roads, building clusters, garden/orchard region, and open fields remain
  recognizable from the tight crop/control family.
- Notebook style is strong: sepia ink, watercolor mottling, muddy roads, roof
  hatching, and dense hand detail.
- No UI, people, animals, church, river, bridge, shop, text, or smoke leakage.

What failed:

- The main cottage roof has an obvious chimney-like nub, failing the absolute
  roof rule.
- Garden and orchard edges still become hard wall/stone-border language in
  several places.
- Tree rows and planting clusters became too regular in the upper field.
- The road junction still composes into a handsome scene more than the source
  crop warrants.

## Interpretation

The control's intent was right, but the artifact still leaves too much edge
structure. Tinting dense planting zones green is not sufficient if the same
control still visibly carries outlines around those zones. The model reads
those outlines as wallable boundaries despite prompt instructions.

The new control also appears to increase vegetation/tree regularity because it
promotes many small dark symbols into a strong planting/tree material channel.

## Current Recommendation

Do not promote BC over BA/BB or AZ.

- `AZ` remains the best visual target for this tight crop, but is edit-target
  evidence only.
- `BA` remains the best fresh no-prior-render recipe attempt.
- `BB` remains the best softened repair from BA.
- `BC` is evidence that the first boundary-material control is too outline-heavy
  and too vegetation-regularizing.

## Next Direction

The next control should be more destructive to wallable garden outlines:

1. Produce a "soft planting mask" that fills dense garden/orchard texture but
   suppresses or blurs its perimeter lines.
2. Separate tree/scrub symbols from garden/internal hatching so tree rows do not
   become regular orchard grids unless the map truly supports that.
3. Keep the raw and cleaned map crops as feature authority, but avoid giving the
   final image model a crisp generated garden border to copy.
4. Consider a repair path only after the fresh render: remove roof nubs and
   soften garden walls as bounded edits, but do not count those as recipe proof.
