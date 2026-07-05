# AC Bounded Notebook Refinement - Cycle AD

Cycle AD starts from the AC direct-control plates and performs one bounded
style/camera repair pass. Unlike AC, this is not a clean one-shot production
candidate because it uses the previous rendered plate as the edit target.

The purpose is diagnostic: can we recover more of the original illustrated
parish notebook's loose ink, uneven watercolor, lower facade readability, muddy
surfaces, and irregular walls/gardens without drifting away from AC's
map-derived topology?

## Inputs

Each site uses:

1. its AC plate as the edit target,
2. the tight local topology control crop as the topology lock,
3. the original historic map crop as source evidence,
4. the original illustrated parish notebook scene as style/camera reference,
5. cleaned single-building slate and thatch crops, and
6. cleaned wall/field/roof material swatches.

## Outputs

| Site         | Output                                                                  | Prompt                                                                        | Report                                                                        | Result                               |
| ------------ | ----------------------------------------------------------------------- | ----------------------------------------------------------------------------- | ----------------------------------------------------------------------------- | ------------------------------------ |
| Beechwood AD | `pipeline-experiments/idea-ad-beechwood-ac-bounded-notebook-refine.png` | `pipeline-experiments/idea-ad-beechwood-ac-bounded-notebook-refine.prompt.md` | `pipeline-experiments/idea-ad-beechwood-ac-bounded-notebook-refine.report.md` | Best Beechwood visual so far from AC |
| Grove AD     | `pipeline-experiments/idea-ad-grove-ac-bounded-notebook-refine.png`     | `pipeline-experiments/idea-ad-grove-ac-bounded-notebook-refine.prompt.md`     | `pipeline-experiments/idea-ad-grove-ac-bounded-notebook-refine.report.md`     | Best Grove visual so far from AC     |

## Audit Questions

- Does AD preserve AC's topology, building count, road/wall continuity, gates,
  yards, gardens, and north-up relationships?
- Does AD reduce garden-row, roof-grid, and stone-wall regularity?
- Does AD make facades, doors, thresholds, wall side faces, and muddy surfaces
  feel closer to the original notebook sample?
- Does AD introduce any forbidden semantic leakage: church, graveyard, bridge,
  river, people, animals, carts, labels, smoke, or chimneys?
- Is any improvement worth the extra repair pass, or should AC remain the
  preferred scalable baseline?

## Result

AD improves the look without destroying AC's tested topology.

Beechwood AD keeps the connected compound, detached top building, lower-right
edge building, roads, gates, walls, and garden enclosure relationships directly
comparable to AC. It is rougher than AC: more ink wobble, muddier yard/road
surfaces, more weathered roofs and limewash, and less board-like cultivated
ground. Some upper-right cultivated beds still read orderly, and far walls can
still look bead-like, but the overall plate is closer to the notebook target.

Grove AD keeps the separate-building yard topology, road curve, detached
eastern thatched building, long southern building, western outbuilding, garden
enclosures, gates, and tree masses directly comparable to AC. It improves paper
tooth, mud, vegetation variation, roof/limewash weathering, and garden
irregularity. The camera is still higher than the original notebook sample and
some boundary walls remain slightly regular.

AD is therefore the current best visual pair, but AC remains the better
production-shaped baseline because it is direct-from-control and does not rely
on a previous rendered plate. The next unresolved problem is not semantic
leakage or door coverage; it is the camera/style gap: getting lower, more
human-scale notebook facades and looser watercolor density in the direct
control path.

## Recommendation

Keep both branches:

- Use AC when evaluating the scalable one-shot/direct-control recipe.
- Use AD as the best current visual reference for what a bounded repair pass can
  achieve after AC.

The next experiment should attack the camera cue directly rather than adding
more style adjectives: provide a stronger reusable low-camera/facade scaffold or
render at a tighter playable crop where buildings occupy more of the frame.
