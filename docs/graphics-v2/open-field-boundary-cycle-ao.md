# Open Field Boundary Cycle AO

## Purpose

Cycle AN improved the original-notebook look and cottage facade readability,
but it still rendered too many ordinary field/parcel boundaries as continuous
stone walls. Cycle AO keeps the same direct map/control path and style
references, but makes one stricter demand: open fields should remain mostly
open, with visible boundary treatment reserved for immediate domestic yards and
the planted garden/orchard enclosure.

This is a direct-control experiment. No prior generated plate is used as an
edit target.

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

## Prompt Change From AN

AO strengthens the open-field rule:

- ordinary field/parcel lines are uncertain and should usually be invisible,
  faint grass shifts, shallow ditches, broken hedge clumps, low banks, scrub,
  or subtle texture,
- roads should not automatically get wall borders,
- visible walls are reserved for immediate domestic yards, building compounds,
  and the planted garden/orchard enclosure,
- the scene should not look like a connected stone-wall network.

## Output

- `pipeline-experiments/idea-ao-kilteevan-open-fields-direct.png`
- `pipeline-experiments/idea-ao-kilteevan-open-fields-direct.prompt.md`
- `pipeline-experiments/idea-ao-kilteevan-open-fields-direct.report.md`

## Result

Cycle AO is the best direct-control signal so far for open-field boundary
restraint. Compared with AN, ordinary open fields remain much softer: most
field divisions are implied by watercolor grass texture, scrub, and subtle
terrain variation instead of continuous stone walls. The central buildings,
upper compound, center-right planted enclosure, broad road network, and
northeast scrub mass remain legible. The deleted admin-boundary failure also
stays controlled; no obvious dotted/suppressed diagonal is restored as terrain.

The important regression is roof artifacts: AO introduces several small
chimneys or chimney-like stacks, despite the hard negative. Treat AO as direct
recipe evidence for open-field softness and topology, not as a clean visual
target until a bounded roof/stub cleanup pass removes those artifacts.

## Audit Questions

- Are open fields visibly softer and less wall-outlined than AN?
- Does AO still preserve the broad lower road, central building frontage, upper
  compound, center-right planted enclosure, and northeast tree/scrub mass?
- Is the deleted diagonal admin-boundary still absent as a physical trace?
- Does the original notebook style remain strong without semantic leakage?
- Are visible buildings readable and equipped with doors/thresholds?
- Are chimneys, chimney-like roof/wall protrusions, and smoke absent?

## Audit Answers

- Open-field softness: pass; clearly improved over AN.
- Topology: partial-to-good pass; major map relationships survive.
- Deleted admin boundary: pass; no obvious restored diagonal terrain trace.
- Notebook style: pass; still a little high, but strong ink/watercolor and
  natural field texture.
- Doors: mostly pass; major visible buildings have readable dark doorways and
  thresholds.
- Chimneys/stubs: fail; several roof stacks or chimney-like artifacts are
  visible.

## Next Step

Run a bounded cleanup only if a clean visual target is needed: remove roof
chimneys/stubs while preserving AO's open-field softness, road topology,
building footprints, doors, and no-admin-boundary behavior. Keep that cleanup
separate from direct recipe evidence.

Cycle AP/AP2/AP3 attempted that cleanup. AP remained visibly incomplete; AP2 is
cleaner, especially on the lower-left roof, but still has a questionable
upper-compound roof mark and some global repaint softness. AP3 removes that
remaining obvious upper-compound roof nub while preserving the inspected doors,
but it still has whole-plate edit/repaint softness. Keep AO as the
boundary/topology recipe evidence and use AP3 only as a downstream visual-target
branch with caveats.
