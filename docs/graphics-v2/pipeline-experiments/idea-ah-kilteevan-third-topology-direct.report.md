# Visual Audit: idea-ah-kilteevan-third-topology-direct

Generated asset: `docs/graphics-v2/pipeline-experiments/idea-ah-kilteevan-third-topology-direct.png`
Prompt sidecar: `docs/graphics-v2/pipeline-experiments/idea-ah-kilteevan-third-topology-direct.prompt.md`
Built-in generator cache source: `/Users/dmooney/.codex/generated_images/019f0d68-9525-7593-ba26-1687b0002b87/ig_0cbc0277107f82be016a40e002dd688196a5f512181798ff94.png`
Format check: PNG, 1672 x 941, RGB, 16:9-ish desktop plate.

## Topology Preservation

Partial pass. The output preserves the big requested structure: a central building group along the lower road, a separate upper center-left enclosed compound, a center-right planted enclosure, a broad lower lane entering from the west/southwest, and a lane rising along the right side of the planted enclosure toward the north/northeast. The central group remains multiple buildings rather than collapsing into one farmhouse, and the upper compound remains a separate yard with small buildings.

The main weakness is that the generated result makes the map crop more orderly and walled than the source evidence supports. Some field divisions have become continuous, high stone walls, and the lower/central road network is a little more junction-like and cleaned up than the historic crop. It is still broadly auditable against the map, but not a strict topology transfer.

## Administrative / Survey Boundary Handling

Partial fail. The bold dotted diagonal line is not copied as dots or a visible labeled artifact, which is good. However, the scene appears to have converted at least one survey-like curving/dotted west-to-center-left boundary into a substantial stone wall across the left/upper-left field. That violates the instruction that dotted or pecked administrative lines should disappear into field texture rather than become walls, hedges, paths, tree rows, or other continuous physical features.

There are no visible map labels, survey numbers, or printed text copied into the scene.

## Roads, Lanes, Buildings, And Enclosures

Mostly pass. Roads and lanes remain coherent: no obvious path runs through a building, no building sits in the middle of a road, and the main yard access reads plausibly. The center-right garden/planted enclosure is coherent and legible, with internal beds and bounded edges. The upper compound and central working yard are readable as distinct farmstead spaces.

The output is somewhat over-built with stone walls and tidy enclosure edges. The garden is more formal and regular than the prompt's "uneven" target, though it still fits the high-confidence planted-area note.

## Door And Threshold Readability

Mostly pass. The main central slate building has a clear dark doorway and threshold. The central outbuildings have readable dark openings or doors, and the upper compound buildings mostly show door-like dark openings facing their yards or access paths.

One upper-compound rear/left small building has a less readable doorway from this camera angle; it may be present as a dark slit, but it is not as clear as the foreground buildings. Overall the door rule is much better satisfied than in many generated map plates, but not flawless.

## Style Match

Good pass. The image lands close to the illustrated parish notebook look: brown/black ink, desaturated watercolor greens and ochres, muddy road scumbling, limewashed stone, rough slate/thatch, irregular vegetation, and paper-tooth texture. It feels like a playable 3/4 orthographic game background rather than a flat survey board.

Style weaknesses: the walls can read a bit too bead-like and strategy-game regular, and the whole scene is slightly cleaner and more composed than the reference notebook plate. The main slate building also has a visible chimney-like stack, which violates the hard negative requesting no chimneys or chimney-like roof nubs.

## Other Hard Negatives

Pass on the major scene negatives: no UI, labels, visible text, people, livestock, carts, water, bridges, churches, graveyards, shops, smoke, or fog are visible. The chimney-like roof stack on the main building is the notable hard-negative miss.

## Overall

Useful but imperfect. It is a strong parish-notebook-style background plate and preserves the core Kilteevan map arrangement well enough for comparison, but it should not be treated as a clean pass for administrative-boundary suppression because at least one likely dotted/survey boundary appears to have become in-world walling.
