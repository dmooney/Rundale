# Lower Rough Close Crop - Cycle S

Cycle S tests whether a direct refinement of Cycle R can recover more of the
original notebook sample's rough hand-drawn character and lower camera while
preserving the successful close crop.

## Inputs

Both Grove and Beechwood used the same generic procedure:

1. Cycle R close-crop output for the same site as the primary crop, topology,
   and scale target.
2. Cycle Q output for the same site as secondary continuity evidence.
3. Cycle M cleaned top-down control as topology authority.
4. Original historic map crop as source evidence.
5. `illustrated-parish-notebook.png` as art-style, roughness, detail-density,
   and lower-camera-feel reference.
6. Deterministic oblique warp of the Cycle M cleaned control as pitch cue.
7. The same cleaned style/material swatches.

The prompt asks for a lower `20-28` degree orthographic camera and stronger
de-regularization of roof grids, stone-wall courses, garden rows, roads, yards,
and building edges.

## Outputs

| Site      | Output                                                                            | Prompt                                                                                  | Report                                                                                  | Result                         |
| --------- | --------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- | ------------------------------ |
| Grove     | `pipeline-experiments/idea-s-grove-lower-rough-close-crop-notebook-style.png`     | `pipeline-experiments/idea-s-grove-lower-rough-close-crop-notebook-style.prompt.md`     | `pipeline-experiments/idea-s-grove-lower-rough-close-crop-notebook-style.report.md`     | Marginal roughness improvement |
| Beechwood | `pipeline-experiments/idea-s-beechwood-lower-rough-close-crop-notebook-style.png` | `pipeline-experiments/idea-s-beechwood-lower-rough-close-crop-notebook-style.prompt.md` | `pipeline-experiments/idea-s-beechwood-lower-rough-close-crop-notebook-style.report.md` | Marginal roughness improvement |

Both returned `1672 x 941` PNGs.

## Verdict

Cycle S is not a decisive endpoint. It improves surface roughness a little, but
the camera remains close to Cycle R and the model largely preserves Cycle R's
regularized isometric solution.

What worked:

- Topology preservation stayed strong on both sites.
- Neither output shows the earlier semantic leaks: church, graveyard, bridge,
  river, shop, UI labels, people, animals, carts, smoke, fog, or random
  chimneys.
- Ground, grass, limewash, roads, and some garden textures gained more tooth,
  staining, and watercolor scumble.

What did not work:

- Roofs still read as polished repeated slate grids.
- Stone walls still often read as tidy courses or bead-like chains.
- Garden rows still look systematic.
- The lower-camera instruction under-delivered; roofs still dominate more than
  facades.

## Lesson

Using Cycle R as the primary reference is excellent for crop and topology, but
it also anchors the model to Cycle R's clean isometric regularity. The next
cycle should provide a stronger style/camera reference that isolates the
original notebook sample's environmental drawing qualities without giving the
model a full UI scene or a polished generated plate to copy.
