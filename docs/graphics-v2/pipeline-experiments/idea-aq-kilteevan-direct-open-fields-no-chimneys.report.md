# Cycle AQ Report - Kilteevan Direct Open Fields No Chimneys

Generated artifact:

- `docs/graphics-v2/pipeline-experiments/idea-aq-kilteevan-direct-open-fields-no-chimneys.png`

Prompt artifact:

- `docs/graphics-v2/pipeline-experiments/idea-aq-kilteevan-direct-open-fields-no-chimneys.prompt.md`

Mode:

- Built-in `image_gen` fresh generation path.
- This was not an edit. No previous generated plate was used as an input, edit target, or layout reference.
- Source cache file copied from `/Users/dmooney/.codex/generated_images/019f0e3c-04ec-7261-aa4a-91b2a2fa3a17/ig_0a29b5bae06b0406016a4115c24c14819585dc286c6fd08277.png`.
- No manual image patching, retouching, resizing, or pixel edits were applied after generation.

Inspection performed:

- Checked final PNG envelope with `file`: `1672 x 941`, RGB PNG, effectively 16:9.
- Viewed the full generated plate.
- Compared visually against `idea-ao-kilteevan-open-fields-direct.png` and `idea-ap3-kilteevan-ap2-upper-roof-nub-cleanup.png`.

## Verdict

AQ is a visually strong fresh notebook-style plate, but a mixed direct-control result. It improves the illustrated-parish-notebook feel and mostly solves AO's chimney/stub failure without needing a cleanup edit. It is worse than AO/AP3 for map/control fidelity and open-field restraint: the result invents a more composed crossroads settlement, adds more buildings/compound emphasis than the crop clearly supports, and uses many low stone-wall fragments around roads and yards. Treat it as useful evidence for style and roof-negative prompting, not as a better one-shot topology recipe than AO.

## Audit

- Topology vs map/control: **mixed / weaker than AO and AP3**. The broad muddy road crossroads and several cottage clusters feel plausible, but the layout reads as a generalized rural crossroads rather than a close read of the supplied Kilteevan controls. It appears to add or regularize buildings and compound relationships beyond the dark roof marks, especially around the lower-left and right-side cottages.
- Open-field softness: **partial fail**. The outer fields remain watercolor-soft and avoid a full chessboard of field outlines, but the scene restores a lot of physical boundary treatment near roads and compounds. There are many connected low stone-wall fragments and gatepost-like marks, more assertive than the prompt's Tier 2/Tier 4 boundary hierarchy intended.
- Deleted/admin-boundary discipline: **mostly pass at full-plate scale**. I do not see an obvious diagonal erased seam or dotted/pecked admin chain restored as a direct terrain scar. The failure is broader wall overuse, not a clear copy of the suppressed admin line.
- Notebook style: **better than AO/AP3**. The loose ink, watercolor vegetation, muddy lane texture, readable facades, paper grain, and varied brushwork are closer to the original illustrated parish notebook target. It is less clean/survey-like than AO/AP3.
- Camera and composition: **good visual plate, weaker control plate**. The camera is playable 3/4 orthographic/isomorphic with readable facades and thresholds. It also feels like a composed game-background scene rather than a north-up transformation of the map crop; the crossroads is pushed into a centered scenic composition.
- Doors per visible building: **mostly pass**. The visible cottages/outbuildings have readable dark openings or door-like faces connected to roads/yards: upper-left main cottage, upper-center outbuilding, lower-left thatched cottage, central slate cottage, lower-left shed, and lower-right small outbuilding. The lower-right small building is the weakest read because vegetation and shadow reduce threshold clarity, but it still appears to have an enterable dark facade.
- Roof/chimney/nub discipline: **better than AO, close to AP3**. I do not see obvious roof chimneys, smoke, roof stacks, or roof-mounted protrusions. Some upright gateposts/wall posts near buildings could read as vertical nubs at thumbnail size, but they appear ground-bound rather than roof-bound.
- Semantic leakage: **pass**. I do not see UI, labels, map text, people, animals, carts, barrels, shopfronts, church/chapel/graveyard content, water, bridge, fog, smoke, or weather effects.
- Better or worse than AO/AP3: **visually better, recipe-control worse**. AQ is better than AO because it has stronger notebook art and no obvious chimneys. AQ is worse than AO for open-field boundary restraint and map-derived layout discipline. AQ is worse than AP3 as a final visual target if the priority is preserving the tested AO/AP topology, because AP3 inherits AO's better direct-control layout and applies a bounded roof cleanup. AQ is useful as a fresh-generation style/roof experiment, but AO plus AP3 remains the better pipeline direction.

## Follow-up Implication

The prompt's no-chimney language worked better in a fresh generation than earlier direct attempts, but the model paid for it by defaulting to a picturesque crossroads with stronger walling. Future direct-control prompts may need to reduce the composition freedom further, or use a tighter map crop/control strategy, if AQ's notebook richness is to be kept without losing AO/AP3's topology.
