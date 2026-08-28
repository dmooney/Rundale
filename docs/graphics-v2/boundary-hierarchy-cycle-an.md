# Boundary Hierarchy Cycle AN

## Purpose

Cycle AM showed that AJ2's cleaned no-admin control can reduce the specific
bold diagonal dot-chain failure, but the render still over-materializes many
thin plot lines as continuous stone walls. Cycle AN tests whether an explicit
boundary-material hierarchy can preserve the deleted-admin-boundary improvement
while making ordinary field and parcel lines softer, broken, or invisible.

## Inputs

- Original map crop:
  `map-sources/kilteevan-z17-map-crop.png`
- Cleaned no-admin control:
  `pipeline-experiments/idea-aj2-kilteevan-dot-suppressed-no-admin-map-crop.png`
- Oblique camera cue:
  `pipeline-experiments/idea-aj2-kilteevan-dot-suppressed-no-admin-oblique-raw-warp.png`
- Full illustrated notebook sample, style only:
  `illustrated-parish-notebook.png`
- Clean single-building slate and thatch style references:
  `style-crops/illustrated-style-low-camera-slate-single-house-door-clean.png`
  and
  `style-crops/illustrated-style-low-camera-thatched-single-house-door-clean.png`
- Tree/field watercolor reference:
  `style-crops/illustrated-style-trees-fields.png`

This cycle deliberately omits the wall-heavy `illustrated-style-fields-walls`
swatch used in AM.

## Prompt Change

The prompt defines a hierarchy:

- suppressed dotted/admin linework: render nothing,
- broad pale corridors: muddy roads/lanes,
- strong enclosed compounds/gardens: optional low broken walls, hedges, banks,
  gates,
- ordinary thin field/parcel lines: soft color changes, shallow ditches,
  broken hedges, overgrown banks, scrub, or no visible mark,
- internal garden strokes: planting texture, not walls.

## Output

Output:

- `pipeline-experiments/idea-an-kilteevan-boundary-hierarchy-direct.png`
- `pipeline-experiments/idea-an-kilteevan-boundary-hierarchy-direct.prompt.md`
- `pipeline-experiments/idea-an-kilteevan-boundary-hierarchy-direct.report.md`

## Result

Cycle AN is a visual improvement over AM. The full notebook sample, constrained
as style-only, helped recover rougher ink/watercolor texture, denser natural
brushwork, and more readable cottage facades without obvious semantic leakage:
no church, water, bridge, UI, people, animals, labels, smoke, or shopfronts are
visible.

Topology is broadly preserved: the broad lower road, central road/building
frontage, upper compound, center-right planted enclosure, and heavier northeast
tree/scrub mass remain readable. The deleted diagonal admin-boundary failure is
still mostly controlled.

The main failure remains wall restraint. AN reduces the diagrammatic feel a bit
but still renders too many enclosure and field edges as continuous stone walls,
especially around the main yard, upper compound, garden, and lower-right field
edge. The next direct-control pass should be stricter: only immediate domestic
yards and the planted enclosure may receive short/broken stone-wall segments;
ordinary open-field parcel lines should usually disappear into grass texture,
ditches, scrub, or very low broken hedges.

## Audit Questions

- Does AN preserve the deleted-diagonal no-trace improvement from AM?
- Are thin/uncertain field lines less wall-like than AM?
- Does the original notebook style improve without semantic leakage from the
  full notebook sample?
- Does topology remain faithful to the map: lower road, central buildings,
  upper compound, center-right planted enclosure, northeast tree/scrub mass?
- Are all visible buildings still equipped with readable doors and thresholds?
- Are chimneys, chimney-like roof/wall protrusions, and smoke absent?

## Audit Answers

- Deleted diagonal boundary: mostly pass; no obvious restored dot-chain wall.
- Boundary hierarchy: partial pass; better than AM stylistically, but still too
  many continuous low stone walls.
- Notebook style: improved; richer ink/watercolor and facade detail.
- Semantic leakage from full sample: pass in this output; no copied church,
  water, bridge, UI, signs, people, animals, or carts.
- Topology: partial pass; major relationships survive, though some roads/edges
  are beautified.
- Doors: mostly pass; visible cottages generally have readable entrances and
  thresholds.
- Chimneys/smoke: pass; no obvious smoke or chimney stacks, though some gate or
  wall posts need continued audit.
