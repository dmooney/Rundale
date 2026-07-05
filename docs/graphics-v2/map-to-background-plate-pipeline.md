# Map To Background Plate Pipeline

This note sketches the broader preferred path for turning historic map crops
into consistent Rundale background plates without per-location prompt hints.
For the active Kilteevan parish exterior workflow, use
`map-to-bu-style-reproducible-pipeline.md`; it is the stricter
subagent-gated version of this pipeline.

## Principle

Do not ask the image model to invent topology from freeform prose. Keep the
historic map crop as the primary visual evidence, and use a reproducible
map-reader stage to add confidence-graded observations about buildings,
boundaries, roads, planting, and negative evidence. The map-reader output may be
location-specific, but the procedure must not be: every crop gets the same
rubric, clean context, and saved note format.

The pipeline should be:

```text
historic map crop + generic legend/characteristic sheet
  -> clean-context map-reader subagent note
  -> clean-context topology/control subagent artifacts
  -> clean-context prompt-builder subagent prompt
  -> clean-context render subagent imagegen call
  -> independent audit subagent checks
```

For recipe evidence, every stage must save its output. A good-looking plate from
one long exploratory context is a candidate, not a reproducible pipeline proof.

## Why A Simple Image Transform Is Not Enough

A perspective or affine warp of the raw map crop can enforce north-up scale and
ground-plane compression, but it keeps the wrong visual content: printed labels,
survey numbers, stipple, map symbols, and flat building marks. It also cannot
create readable facades, doors, roofs, walls, gates, hedges, or tree volumes.

The better transform is not "warp the old map into perspective"; it is "read the
map's physical features, preserve uncertainty, then guide the renderer with
auditable observations." Early experiments suggest that raw-map generation plus
small style swatches is safer than full-scene references, and Cycle K suggests a
reproducible map-reader note can improve building interpretation without
hard-coding a route graph.

## Reproducible Map Reader Targets

The map-reader pass should produce these observations with confidence scores:

- building inventory from dark solid/hatched rectangles and rectangular marks,
  including relative position, footprint shape, evidence, probable function,
  confidence, and renderer notes,
- roads and lanes from paired parallel lines, road-width corridors, and
  route-continuity cues,
- boundaries from single, dotted, dashed, pecked, or broken linework,
- administrative/survey boundaries from dotted, dashed, pecked, or dot-chain
  linework that does not have independent physical evidence; these must be
  recorded as non-physical or ambiguous ignore marks rather than terrain,
- explicit negative evidence for church, shop, water, bridge, UI/text, smoke,
  and other high-risk hallucination targets,
- trees from broadleaf/conifer symbols and density/context,
- planted enclosures from regular internal divisions and repeated tree/bed
  symbols,
- text, numbers, survey marks, scan noise, and modern overlays as ignore marks.

Road/wall/water classification is the brittle part. The reader must preserve
uncertainty instead of forcing a definitive class. Do not solve ambiguity by
writing hand hints for a single location; rerun the same rubric, compare notes,
or mark the feature as ambiguous.

## Control Plate

Optional control plates can still be useful, but they are not required for this
pipeline to be reproducible. Cycle L showed that a top-down cleaned control
plate can improve central topology before final rendering. The main unresolved
issue is native 16:9 framing without synthetic edge extension. When used, render
controls from the original map plus map-reader observations:

- fixed output aspect ratio and pixels-per-meter,
- fixed ground-plane transform for every location,
- north stays at final-image top,
- a top-down cleaned plate or linework scaffold as the primary
  topology/perspective guide,
- roads as walkable corridors with consistent width by class when confidently
  detected,
- boundaries as conservative low walls, hedges, ditches, or fence candidates,
  but only when the linework has physical evidence; non-physical
  administrative or survey boundaries should not be drawn,
- building footprints as soft semantic hints first,
- buildings as extruded boxes with roof volumes and visible facades only after
  extraction confidence is high,
- gates/openings where roads cross boundaries,
- trees as simple canopy markers beside routes, not in route centers,
- optional depth/height/semantic masks if the image model supports control
  inputs.

The original map crop remains the main spatial reference. The map-reader note
and any cleaned control plate or linework scaffold are secondary
topology/perspective aids and verification targets. Semantic masks are soft
class hints. Style references should be cropped texture/material/camera
snippets, not full scenes with distinctive landmarks.

## Image Generation Contract

The image model receives:

- the original historic map crop,
- the generic legend or characteristic sheet,
- the reproducible map-reader note for that crop,
- optional control plate/masks,
- approved cleaned style/material swatches,
- the generic prompt.

It does not receive:

- per-location route descriptions,
- hand-authored road notes,
- human guesses about walls, rivers, buildings, or exits,
- map-reader notes produced by a different rubric or contaminated context,
- previous failed/generated plates.

The render runner should be a clean-context render subagent that calls image
generation with only the declared inputs. If tool limitations force the
coordinator to call imagegen, record that as an exception and treat the result
as weaker recipe evidence. The independent auditor should be a separate
subagent that did not build the prompt. Use at most one bounded correction for a
concrete audit failure; keep direct recipe evidence separate from edited
visual-target evidence.

## Automated Checks

After generation, compare the plate back to the map and the map-reader notes:

- roads/lanes described by the map-reader note remain visible and unblocked,
- extra walkable-looking paths are below a tolerance,
- buildings remain near observed footprints and preserve likely grouping,
- gates/openings line up with route crossings,
- no text, labels, map pins, visible smoke, or invented water appear,
- camera metrics stay fixed: same sprite scale, same building facade/roof ratio,
  same north-up ground-plane transform.

Failed checks should first ask whether the map-reader note over-read or
under-read the crop. Rerun the same rubric or record uncertainty; do not patch
the render prompt with hand-authored special cases.

## Current Accuracy Signal

Cycle K remains the simpler one-step baseline. Cycle M is the leading two-step
candidate because it keeps Cycle L's top-down control benefits and adds an
explicit administrative/survey boundary ignore class. This prevented
Beechwood's unsupported dotted eastern boundary from becoming a fake hedge or
wall. Do not promote the two-step path to the default batch path until the
first top-down stage reliably produces exact native 16:9 plates without
padding, cropping, or edge extension.
