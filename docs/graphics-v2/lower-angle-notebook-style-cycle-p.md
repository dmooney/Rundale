# Lower-Angle Notebook Style - Cycle P

Cycle P tests whether the Cycle M two-step topology can keep its map accuracy
while moving the final render closer to the original illustrated parish notebook
camera and art style.

## Inputs

Both Grove and Beechwood used the same generic procedure:

1. Cycle M cleaned top-down control plate for the site.
2. Previous notebook-style isomorphic output for the same site as a secondary
   continuity reference.
3. Original historic map crop as source evidence.
4. `illustrated-parish-notebook.png` as a full-scene style and camera-feel
   reference only.
5. The same cleaned material swatches:
   - `style-crops/illustrated-style-field-wall-no-animals.png`
   - `style-crops/illustrated-style-wall-roof-no-props.png`

The full-scene notebook reference is allowed here only because the prompt
strongly labels it as style/camera-only and explicitly forbids copying its UI,
people, church, chapel, shop, bridge, river, labels, smoke, carts, animals, and
landmarks.

## Outputs

| Site | Output | Prompt | Report | Result |
| --- | --- | --- | --- | --- |
| Grove | `pipeline-experiments/idea-p-grove-lower-angle-notebook-style.png` | `pipeline-experiments/idea-p-grove-lower-angle-notebook-style.prompt.md` | `pipeline-experiments/idea-p-grove-lower-angle-notebook-style.report.md` | Best camera/style pass so far |
| Beechwood | `pipeline-experiments/idea-p-beechwood-lower-angle-notebook-style.png` | `pipeline-experiments/idea-p-beechwood-lower-angle-notebook-style.prompt.md` | `pipeline-experiments/idea-p-beechwood-lower-angle-notebook-style.report.md` | Generalizes to second crop with caveats |

Both returned `1672 x 941` PNGs. This is near 16:9, but still not mathematically
exact native 16:9.

## What Changed

Cycle N showed that adding the original notebook scene as a style reference
improved the art direction: darker roof hatching, rougher sepia contours,
mottled olive fields, dirtier limewash, muddy roads, and more paper-grain
texture.

Cycle O and Cycle P then pushed the camera lower. The Cycle P prompt makes the
camera requirement concrete:

- low 3/4 orthographic oblique game camera,
- roughly 35-40 degrees above the ground plane,
- camera positioned south of the scene looking north,
- geographic north still stays at the top of the image,
- substantial visible facades, doors, wall faces, side walls, gate posts, tree
  trunks/lower masses, and stone-wall side faces,
- no horizon, sky, vanishing point, drone view, rotated composition, or
  top-down survey angle.

The key wording is that building facades should read as roughly `40-60%` of the
visible roof depth on main buildings. This gave the model a more concrete
target than simply saying "lower isometric camera."

## Verdict

Cycle P is the current best direct final-render candidate for matching the
original illustrated parish notebook style while preserving Cycle M's
map-layout accuracy. Cycle Q later refines its camera further; see
`camera-refinement-cycle-q.md`.

What worked:

- Grove has the best lower-angle camera so far: stronger facades, clearer door
  faces, more building height, readable wall sides, and a less survey-like
  feel.
- Beechwood preserved the diagonal road, building group, planted enclosure,
  woodland, southern outbuildings, and suppression of the unsupported eastern
  administrative dotted boundary.
- Both plates keep the rougher notebook family: sepia ink, dark broken roof
  hatching, mottled watercolor fields, muddy roads, limewashed walls, and
  paper-grain texture.
- Neither plate shows the earlier high-risk leaks: church, graveyard, bridge,
  river, shop, smoke, UI labels, people, animals, carts, or random freestanding
  chimneys.

What still needs work:

- Beechwood remains more survey-like than the ideal low playable camera,
  especially in the planted enclosure and garden rows.
- Garden rows and several stone walls are still cleaner and more regular than
  the original notebook sample.
- Some boundaries may be rendered as stronger continuous stone walls than the
  map evidence strictly supports.
- Exact native 16:9 remains unresolved.

## Current Recommendation

Use Cycle P as the leading direct final-render style/camera prompt after the
Cycle M topology path:

```text
historic map crop
  -> clean-context reproducible map-reader note
  -> top-down cleaned control plate with admin-boundary ignore class
  -> lower-angle notebook-style isomorphic plate
```

For the next production-style test, keep the Cycle P camera block and test a
third map crop that has a different building density. If the camera keeps
drifting high, the next pipeline change should be a deterministic oblique
control/blockout image with explicit facade volumes, not just stronger wording.
