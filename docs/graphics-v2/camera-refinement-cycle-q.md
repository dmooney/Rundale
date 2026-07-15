# Camera Refinement - Cycle Q

Cycle Q refines the Cycle P notebook-style plates by using a previous successful
plate as the primary style/topology target and a deterministic oblique warp only
as a camera-pitch cue.

The goal is not to reinterpret the map. The goal is to keep Cycle M/P topology
and lower the final camera enough that facades, thresholds, wall sides, and tree
lower masses read like a playable 3/4 environment rather than a high survey
plate.

## Inputs

Both Grove and Beechwood used the same generic procedure:

1. Cycle P output for the same site as the primary style/topology target.
2. Cycle M cleaned top-down control plate as the topology authority.
3. Original historic map crop as source evidence.
4. `illustrated-parish-notebook.png` as style and low-camera-feel reference.
5. A deterministic oblique warp of the Cycle M cleaned control, used only as a
   camera-pitch cue.

The oblique warp is not a content source. It is only there to show the model how
the north-south ground plane compresses when the camera is actually lowered.
The prompt explicitly rejects the warp's beige margins, strip composition,
texture artifacts, and exact crop.

## Outputs

| Site      | Output                                                                       | Prompt                                                                             | Report                                                                             | Result                                  |
| --------- | ---------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- | --------------------------------------- |
| Grove     | `pipeline-experiments/idea-q-grove-camera-refinement-notebook-style.png`     | `pipeline-experiments/idea-q-grove-camera-refinement-notebook-style.prompt.md`     | `pipeline-experiments/idea-q-grove-camera-refinement-notebook-style.report.md`     | Best Grove camera/style pass so far     |
| Beechwood | `pipeline-experiments/idea-q-beechwood-camera-refinement-notebook-style.png` | `pipeline-experiments/idea-q-beechwood-camera-refinement-notebook-style.prompt.md` | `pipeline-experiments/idea-q-beechwood-camera-refinement-notebook-style.report.md` | Best Beechwood camera/style pass so far |

Both returned `1672 x 941` PNGs. Exact native 16:9 remains unresolved.

## What Changed

Cycle P proved that stronger language about a low 3/4 orthographic camera helps,
but Beechwood still looked too survey-like in the garden beds.

Cycle Q keeps the Cycle P output visible and asks for a camera-refinement pass
rather than a new map interpretation. It adds a deterministic oblique warp as a
pitch cue, with strict instructions not to copy its composition or artifacts.

The prompt focuses on:

- lower 30-35 degree oblique orthographic camera,
- north-up ground plan,
- camera south of the scene looking north,
- no horizon, sky, vanishing point, drone view, fisheye, or rotation,
- prominent limewashed facades, doors, windows, thresholds, side walls, damp
  stone bases, wall side faces, tree trunks, and dark lower tree masses,
- topology invariants from the Cycle M control plate,
- no new landmarks, roads, water, churches, shops, people, carts, labels,
  smoke, or chimneys.

## Blockout Lesson

The existing `scripts/prototype_map_controls.py` can generate oblique warps and
rough extruded blockouts without new dependencies. Running it on the Cycle M
cleaned controls produced useful oblique pitch cues, but the heuristic
blockouts over-detected texture as buildings. Do not promote those blockouts to
the recommended path yet.

The useful signal is narrower: a deterministic oblique warp can communicate
camera compression, while the successful rendered plate and Cycle M top-down
control keep the content stable.

## Verdict

Cycle Q is the current best candidate for the objective:

```text
replicate the original illustrated parish notebook style
while preserving Cycle M's map-layout accuracy
```

What worked:

- Grove and Beechwood both have stronger facades, door/threshold readability,
  wall side faces, tree trunks/lower masses, and a lower playable camera feel
  than Cycle P.
- Both retain the rough notebook visual family: heavy sepia contours, broken
  slate roof hatching, dirty limewash, mottled olive watercolor fields, muddy
  road scumbling, dry-brush stone, cool shadows, and paper-grain staining.
- Both preserve the main Cycle M topology for their crop: roads, yards,
  building clusters, planted enclosures, tree masses, and omission of
  unsupported administrative dotted boundaries.
- Neither output shows the earlier semantic leaks: church, graveyard, bridge,
  river, shop, visible smoke, UI labels, people, animals, carts, or random
  chimneys.

Remaining caveats:

- Garden rows still read more plan-like than the ideal low-camera treatment.
- Some walls are cleaner and more continuous than the original notebook's rough
  broken-wall feel.
- Grove's small working-yard buildings may read slightly busier than the Cycle
  M control.
- Exact native 16:9 is still not solved.

## Current Recommendation

Use Cycle Q as the leading final-render path:

```text
historic map crop
  -> clean-context reproducible map-reader note
  -> Cycle M-style top-down cleaned control with admin-boundary ignore class
  -> Cycle P/Q-style notebook plate using the original notebook style reference
  -> camera refinement with prior successful plate + top-down control + oblique pitch cue
```

For batch production, run at least one more crop through Cycle Q before freezing
the recipe. If camera drift persists, the next pipeline improvement should be a
proper deterministic low-angle semantic blockout that uses reliable feature
classes, not the current rough connected-component blockout.
