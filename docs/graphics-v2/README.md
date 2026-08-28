# Graphics V2 Research Index

> Status: exploratory visual research. These are concept, pipeline, and
> provenance artifacts; they are not shipped runtime assets. Follow
> [`AGENTS.md`](AGENTS.md) for the full preservation and clean-context rules.

This directory is the entry point for the illustrated-notebook visual direction:
the visual play surface, background-plate research, concept interiors, portraits,
and map-interpretation evidence. It deliberately separates current product work
from longer-term visual-client alternatives so agents do not mix their constraints.

## Start with the task at hand

| If you need to...                                           | Read this first                                                                         | Then use                                                                                                                                                 |
| ----------------------------------------------------------- | --------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Work on responsive art in the shipped chat surface          | [Chat-first stabilization contract](../../parish/apps/ui/CHAT_FIRST_STABILIZATION.md)   | [GUI features](../features.md#chat-first-illustrated-viewport)                                                                                           |
| Understand the earlier Svelte notebook proposal             | [Parish Notebook UI](../design/parish-notebook-ui.md)                                   | Treat it as superseded in implementation detail by the Pixi-oriented real-play-screen plan above.                                                        |
| Resume a background-plate experiment                        | [current Q/M handover](handover-2026-06-28-notebook-qm.md)                              | [reproducible BU-style pipeline](map-to-bu-style-reproducible-pipeline.md) and the [Kilteevan run template](kilteevan-exterior-pipeline-run-template.md) |
| Interpret a historic map before rendering                   | [map-reader stage template](map-reader-stage-template.md)                               | [map annotator](map-annotator/README.md) and [OS map-key references](web-references/os-6inch-map-key/README.md)                                          |
| Build a playable visual location rather than another render | [runtime layers and independent variables](runtime-layers-and-independent-variables.md) | [Interactive Parish Diorama RFC](../design/ideas/parish-diorama.md) and its [implementation plan](../plans/parish-diorama-implementation.md)             |
| Explore a separate Godot presentation client                | [Godot-based Rundale plan](../design/godot-parish-game-plan.md)                         | This is a proposed client direction, not the current chat implementation.                                                                                |

## Research map

- **Exterior pipeline:** [map-to-background-plate pipeline](map-to-background-plate-pipeline.md) records the earlier research; [map-to-BU reproducible pipeline](map-to-bu-style-reproducible-pipeline.md) is the current reusable procedure. Use [Irish dry-stone-wall reference](irish-dry-stone-wall-reference.md) whenever an exterior could contain a real stone wall.
- **Evidence and comparison:** [cartographic comparisons](cartographic-comparisons/README.md) and the [pipeline experiments](pipeline-experiments/README.md) preserve source/control/render chains through tracked sidecars and the external archive index. Do not treat an edited visual target as fresh-render recipe proof.
- **Canonical clean-checkout inputs:** [map sources](map-sources/README.md) retain the three map crops consumed by current tools, while [authorities](authorities/README.md) retains the accepted BU E2 style target.
- **Visual language:** [style crops](style-crops/README.md) identifies safe reusable references and superseded, leaky ones. The original [notebook concept](illustrated-parish-notebook.png) and [environment-only plate](illustrated-parish-scene-no-ui.png) are direction references, not asset sheets.
- **Interior concepts:** [interior-concepts](interior-concepts/README.md) records the small playable cutaway targets and historical anchors.
- **Portraits:** [NPC portrait workflow](npc-portraits/README.md) describes experiments, approval, derivatives, and manifest ownership.
- **Overhead art:** [overhead-art](overhead-art/README.md) contains map/pawn concepts that are distinct from the low-oblique exterior pipeline.

## Decision boundaries

- The current default client is the semantic chat shell. The retired Pixi
  notebook, Parish Diorama, and Godot documents are historical or related
  proposals, not implementation authority for that work.
- Historic maps are provenance and geometry evidence for exterior research. The
  current runtime notebook design explicitly disallows using them as runtime
  image references.
- Generated plates are stable base layers. Time, weather, actors, props, and
  interaction state should be runtime layers where possible; see
  [runtime-layers-and-independent-variables.md](runtime-layers-and-independent-variables.md).
- Keep every prompt, report, and audit beside the artifact's archive-relative
  name, and index each archived control, comparison, and render by path and hash.
  This makes a result independently reviewable without making every clone carry
  the binary corpus.
