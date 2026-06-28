# Cycle AR Report - Kilteevan Tight Control No Scenic Crossroads

Generated artifact:
- `docs/graphics-v2/pipeline-experiments/idea-ar-kilteevan-tight-control-no-scenic-crossroads.png`

Prompt artifact:
- `docs/graphics-v2/pipeline-experiments/idea-ar-kilteevan-tight-control-no-scenic-crossroads.prompt.md`

Mode:
- Built-in `image_gen` fresh generation path.
- This was not an edit. No previous generated plate was used as an input, edit target, style target, layout reference, or composition reference.
- Source cache file copied from `/Users/dmooney/.codex/generated_images/019f0e55-43ed-72d2-8cfe-fa0f6c265551/ig_061fd6940e759108016a411c2984d48194ae6db2988a67055b.png`.
- No manual image patching, retouching, resizing, or pixel edits were applied after generation.

Inspection performed:
- Checked final PNG envelope with `file`: `1672 x 941`, RGB PNG, effectively 16:9.
- Viewed the full generated plate after copying it into the workspace.
- Reviewed the existing AO, AP3, and AQ reports for comparison.

## Verdict

AR is a useful but mixed fresh-generation result. The tighter crop and cleaned control improve the direct-control direction over AQ: the image is less sprawling, preserves a closer playable scale, keeps larger open field areas soft, and retains AQ's no-chimney discipline without needing an edit pass. It still does not fully solve the scenic-composition problem: the roads have been regularized into a centered, picturesque Y/crossroads, with a bit too much road/yard boundary structure for a truly awkward map-derived crop.

## Audit

- Topology vs tight map/control: **partial pass, modestly better than AQ**. The plate reads as a tight local area rather than a zoomed-out village, and the building groups stay in a plausible relationship to the muddy roads and open fields. However, the generated layout still simplifies the map into a centered crossroads/Y-intersection and regularizes the road corridors more than the source crop warrants.
- North-up / perspective: **pass** at visual inspection scale. The ground plan is not obviously rotated, and the 3/4 orthographic game-camera feel is readable, with facades and thresholds visible.
- Open-field softness: **mostly pass**. The right and lower open fields stay broad and watercolor-soft, with scrub and grass texture rather than a field-wall chessboard. Some road and yard edges are still over-articulated with wall/fence fragments, so this is not as restrained as the ideal AO-style open-field rule.
- Deleted/admin-boundary discipline: **pass at full-plate scale**. I do not see an obvious restored diagonal erased scar, dotted survey chain, or admin-boundary trace. The boundary issue is general scenic yard/road outlining, not a clear resurrection of the suppressed linework.
- Roof/chimney/nub discipline: **pass**. I do not see visible chimneys, smoke, roof stacks, roof pegs, or chimney-like roof nubs. The slate and thatch roofs read as continuous textured planes.
- Doors and thresholds: **pass**. The visible cottages/outbuildings have readable dark timber doorways on visible facades, with thresholds or small approaches connected to yards/roads. The top-left slate cottage, top-right thatched cottage, lower-left small outbuilding, and lower-left larger cottage all read as enterable.
- Semantic leakage: **pass**. No UI, labels, map text, people, animals, carts, barrels, church/chapel/graveyard, shopfront, water, bridge, fog, smoke, or weather effects observed.
- Notebook style: **good**. The loose ink, mottled watercolor fields, muddy road texture, rough vegetation, limewashed facades, and paper-grain atmosphere are closer to the notebook target than the cleaner survey-plate failures.

## Comparison

- Against AO: **better on roof discipline and notebook detail; mixed/worse on composition restraint**. AO had stronger open-field/topology evidence but failed with visible chimney/stub artifacts. AR fixes the roof problem in one fresh pass, but its centered road composition is still more scenic than AO's best direct-control reading.
- Against AP3: **better as one-shot evidence; probably worse as a final visual target**. AP3 is an edit cleanup, so AR is cleaner recipe evidence. But AP3 inherits the AO/AP topology and applies a bounded roof fix, making it stronger if the priority is a final controlled Kilteevan plate.
- Against AQ: **better overall for the next direct-control experiment**. AR keeps the useful AQ no-chimney language while reducing AQ's sprawling generic settlement drift and extra-building feel. It still has residual crossroads beautification, so the improvement is real but not decisive.

## Follow-up Implication

The tighter crop helped, but the prompt/control stack still gives the model too much permission to compose roads into a neat game crossroads. The next direct-control pass should probably constrain road geometry even harder, or use a stronger topology control/mask, while preserving this cycle's no-chimney language and crop scale.
