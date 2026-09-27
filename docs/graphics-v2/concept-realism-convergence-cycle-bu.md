# Concept Realism Convergence Cycle BU

## Purpose

Cycle BU continues the BS/BT relaxed concept-art branch until the rendered
Beechwood plate feels matched to the original illustrated parish notebook's
environment realism, within the constraints of:

- no UI, labels, people, smoke, animals, or carts,
- no new church/shop/river/bridge/village-crossroads content,
- BS E2's door-height and playable zoom target,
- connected Beechwood compound topology,
- readable fitted plank doors on every visible walkable facade.

The aim is a visual target, not a fresh production recipe. BU uses prior
rendered plates as edit targets, so it should not be treated as proof that a
clean one-shot map-to-art pipeline is solved.

## Setup

BU E1 renders the BT recommendation directly:

```text
BT E2 sparse practical clutter + BT E3 irregular garden/wall/road edges,
with an explicit cap on repeated buckets/barrels and a lighter watercolor
value range than BT E3.
```

BU E2 then makes one minimal tightening pass from BU E1:

```text
Remove repeated container patterns, soften estate-plan wall/garden regularity,
keep the warm value range, and preserve doors/topology/zoom.
```

## Outputs

| ID  | Image                                                                 | Prompt                                                                          | Report                                                                          | Result                                      |
| --- | --------------------------------------------------------------------- | ------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- | ------------------------------------------- |
| E1  | `pipeline-experiments/idea-bu-e1-bt-e2-e3-concept-realism-hybrid.png` | `pipeline-experiments/idea-bu-e1-bt-e2-e3-concept-realism-hybrid.prompt.md`     | `pipeline-experiments/idea-bu-e1-bt-e2-e3-concept-realism-hybrid.report.md`     | Good hybrid, but repeated containers remain |
| E2  | `authorities/beechwood-concept-realism-bu-e2.png`                     | `pipeline-experiments/idea-bu-e2-bu-e1-concept-realism-final-tighten.prompt.md` | `pipeline-experiments/idea-bu-e2-bu-e1-concept-realism-final-tighten.report.md` | Accepted convergence point                  |

Comparison plate:

- `cartographic-comparisons/bu-concept-realism-convergence-comparison.png`

## Verdict

BU E2 is the best current match to the original concept-art material language:
warm paper, rough ink, worn limewash, muddy roads, scuffed thresholds, practical
yard details, handmade vegetation, and irregular but readable walls/gardens.

The remaining difference is mostly subject matter. The original notebook image
is a village crossroads with UI labels, people, a shop/church setting, and
busier civic detail. BU E2 is a no-UI Beechwood compound plate. Within that
quieter source/location constraint, the art direction is close enough that I
would stop rather than run another open-ended polish pass.

## Recommendation

Use BU E2 as the relaxed concept-art visual target for this branch. For future
recipe work, copy the prompt lessons rather than the edit dependency:

- gate scale by concept-art door height,
- require fitted plank doors on every person-sized opening,
- use sparse practical clutter rather than generalized dirt,
- cap repeated buckets/barrels/containers,
- soften regular gardens and walls without turning every boundary into a heavy
  enclosure,
- stop once the plate is warm, worn, readable, and topology-stable.
