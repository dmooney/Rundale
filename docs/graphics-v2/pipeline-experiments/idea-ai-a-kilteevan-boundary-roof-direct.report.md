# Visual Audit: idea-ai-a-kilteevan-boundary-roof-direct

Generated asset: `docs/graphics-v2/pipeline-experiments/idea-ai-a-kilteevan-boundary-roof-direct.png`
Prompt sidecar: `docs/graphics-v2/pipeline-experiments/idea-ai-a-kilteevan-boundary-roof-direct.prompt.md`
Built-in generator cache source: `/Users/dmooney/.codex/generated_images/019f0d74-f0af-7791-b354-0ac70b9147ab/ig_081cc5743c52b31c016a40e2c8475881958919236bcad6644c.png`
Format check: PNG, 1672 x 941, RGB, 16:9-ish desktop plate.

## Topology Preservation

Partial pass. The generated scene keeps the main Kilteevan arrangement readable: a central road-front building cluster, a separate upper center-left enclosed compound, a center-right planted enclosure, lower-half muddy lanes, and a denser tree/scrub mass toward the upper right. The central cluster remains multiple buildings instead of collapsing into a single farmhouse, and the upper compound remains a separate yard with small structures.

The weak point is over-regularization. Several thin or ambiguous line relationships became continuous, well-built stone walls, and the road network is more expanded and formal than the source crop. The broad lower lane and the center-right lane remain plausible, but the output feels more like a cleaned game map than a cautious rendering of confidence-graded map evidence.

## Administrative Boundary No-Trace

Fail. The prompt's no-trace rule was stronger than the previous AH prompt, but the generated plate still leaves continuous physical features along likely non-physical boundary courses. The west and upper-left curving survey-like line appears to have become a long stone wall. The strong right-side diagonal/upper-right course is also at least partly materialized as a continuous road or wall-edge corridor rather than dissolving into ordinary field texture.

The image does not copy dotted map marks, labels, survey numbers, or visible text, which is good. The problem is semantic conversion: non-physical linework has been turned into traceable in-world walling or road geometry.

## Chimneys And Roof Protrusions

Pass. I do not see a chimney, smoke plume, ridge stack, vent, roof nub, or decorative roof cap on the central slate roof, the thatched outbuildings, or the upper-compound buildings. Roofs read as uninterrupted slate or thatch with moss/patching texture only.

There are gate posts and entrance posts in the scene, especially near yard and field openings. Those are not roof protrusions, and they are consistent with the camera prompt's request for readable gates.

## Door And Facade Readability

Mostly pass. The main central slate building has a clear dark doorway and threshold facing the lane/yard. The foreground and central thatched outbuildings also have readable dark entrances. The upper compound buildings have at least one visible doorway or dark threshold facing inward to the yard.

The one caveat is the small upper-compound building on the left side: its doorway is less explicit from this camera angle. It may be occluded or facing inward, but it is not as confidently readable as the central cluster.

## Style Match

Good pass. The plate has the intended illustrated parish-notebook feeling: hand-inked texture, desaturated watercolor greens and ochres, scumbled muddy roads, limewashed stone, rough slate and thatch, irregular vegetation, and paper-tooth surface texture. It reads as a playable 3/4 orthographic background, with roofs and facades both visible.

Style weaknesses are mostly tied to topology: the stone walls are too continuous and bead-like in places, the planted enclosure is a little too formal, and the roads/walls are cleaner than the rough notebook target. There is no UI, border, label, sign, person, livestock, cart, shop, church, graveyard, bridge, water, smoke, or fog visible.

## Overall

Useful as a roof-protrusion cleanup test and style sample, but not a clean pass for the boundary experiment. Compared with `idea-ah-kilteevan-third-topology-direct`, this removes the obvious chimney failure, but it still fails the administrative-boundary no-trace requirement by turning likely survey/admin courses into continuous physical features.
