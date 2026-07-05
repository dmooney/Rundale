# Close-Crop Notebook Style - Cycle R

Cycle R tests the user's hypothesis that the original illustrated parish
notebook sample feels richer partly because it depicts a smaller playable area
at the same pixel size.

The goal is not to reinterpret the historic map or replace Cycle M/Q topology.
The goal is to keep the Cycle M/Q map-derived layout and spend more pixels on
the local play space: facades, doors, thresholds, muddy yard surfaces, wall
faces, garden edges, roof hatching, and handmade watercolor texture.

## Inputs

Both Grove and Beechwood used the same generic procedure:

1. Cycle Q output for the same site as the primary style/topology continuity
   target.
2. Cycle M cleaned top-down control plate as the topology authority.
3. Original historic map crop as source evidence.
4. `illustrated-parish-notebook.png` as style, scale, detail-density, and
   lower-camera-feel reference only.
5. A deterministic oblique warp of the Cycle M cleaned control, used only as a
   camera-pitch cue.
6. The same cleaned material swatches:
   - `style-crops/illustrated-style-field-wall-no-animals.png`
   - `style-crops/illustrated-style-wall-roof-no-props.png`

The prompts stay generic: Beechwood uses `target location` wording rather than
hand-authored Beechwood-specific road, wall, or building notes. The map/control
images provide the location evidence.

## Outputs

| Site      | Output                                                                | Prompt                                                                      | Report                                                                      | Result                              |
| --------- | --------------------------------------------------------------------- | --------------------------------------------------------------------------- | --------------------------------------------------------------------------- | ----------------------------------- |
| Grove     | `pipeline-experiments/idea-r-grove-close-crop-notebook-style.png`     | `pipeline-experiments/idea-r-grove-close-crop-notebook-style.prompt.md`     | `pipeline-experiments/idea-r-grove-close-crop-notebook-style.report.md`     | Strong close-crop scale/detail pass |
| Beechwood | `pipeline-experiments/idea-r-beechwood-close-crop-notebook-style.png` | `pipeline-experiments/idea-r-beechwood-close-crop-notebook-style.prompt.md` | `pipeline-experiments/idea-r-beechwood-close-crop-notebook-style.report.md` | Generalizes the close-crop method   |

Both returned `1672 x 941` PNGs.

## What Changed

Cycle Q improved camera consistency, but both sites still read as broad survey
plates. Cycle R changes the output composition, not the source evidence:

- keep the same 16:9 frame,
- cover much less ground,
- focus on the central building cluster and immediate playable yard/garden
  edges,
- allow roads, walls, and boundaries to continue off-frame,
- omit distant source-map context instead of miniaturizing everything,
- keep the low orthographic camera and north-up ground plan from Cycle Q,
- push rougher ink/watercolor material language.

This produces larger, more readable facades, doors, roof texture, yard scumble,
stone walls, gates, garden edges, and tree masses without asking the model to
invent new layout.

## Verdict

Cycle R is a useful next step after Cycle Q. The scale/detail problem improved
on both Grove and Beechwood, and neither output shows the earlier high-risk
semantic leaks: church, graveyard, bridge, river, shop, UI labels, people,
animals, carts, visible smoke, or random freestanding chimneys.

What worked:

- Both outputs feel more playable and human-scale than Cycle Q.
- Doors, thresholds, wall faces, and muddy yard/road surfaces are readable
  enough for sprite staging.
- The outputs remain close to Cycle M/Q topology while trimming distant survey
  context.
- Grove and Beechwood now look more consistent with one another as possible
  background plates.

What still needs work:

- The camera is still a little higher than the original illustrated notebook
  sample.
- Garden rows, roof tiles, and stone-wall courses remain cleaner and more
  systematic than the rough hand-drawn sample.
- The model still tends to regularize buildings and walls into polished
  isometric-game shapes when the control images are clean.

## Current Recommendation

Use Cycle R as the leading composition strategy when the goal is the original
notebook feel:

```text
historic map crop
  -> clean-context reproducible map-reader note
  -> Cycle M-style top-down cleaned control with admin-boundary ignore class
  -> Cycle Q-style notebook plate with oblique pitch cue
  -> Cycle R close playable crop around the local building/yard cluster
```

For the next cycle, keep the close-crop policy but test one of:

- a tighter source/control crop generated before the final render, rather than
  asking the model to crop from a wide Cycle Q plate,
- stronger roughness language that explicitly de-regularizes garden rows,
  stone-wall courses, roof tiles, and building edges,
- a lower deterministic oblique pitch cue with less visible top-down ground
  plane.
