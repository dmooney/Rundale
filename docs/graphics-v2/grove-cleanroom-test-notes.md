# Grove Cleanroom Test Notes

These notes record the current one-shot prompt experiments for turning a
historic Grove map crop into a walkable 3/4 orthographic isometric background
plate.

## Current Findings

The prompt must be tested in a clean context. Same-thread image generations are
not reliable evidence because prior failed renders can bias later calls. Use
fresh subagents or fresh model sessions and attach only the intended references.

The biggest control lever is not another paragraph of prompt text; it is the
source map extent. The original Grove crop was arbitrarily wide and repeatedly
encouraged survey-map compositions. A tighter target-site map crop around Grove
made the same prompt much more reliable.

Best current recipe:

- choose plate scale, sprite scale, and 3/4 orthographic camera first,
- back-calculate or crop the historic map window around the named site's local
  playable area and exits,
- keep the map crop and generated plate north-up,
- keep the reusable prompt generic,
- do not pass location-specific interpretation notes,
- if the raw map crop is not enough, generate a repeatable CV/vector control
  image rather than hand-reading roads, walls, rivers, or buildings,
- attach that target-site map crop as layout reference,
- attach the approved illustrated notebook render as style/camera reference,
- use `portable-background-plate-one-shot-template.md`,
- test in fresh subagents / fresh image sessions.

## Test Outputs

- `grove-cleanroom-subagent-maponly-a.png`
  - Inputs: Grove map crop only.
  - Strengths: no visible smoke, no invented water, no freestanding
    roof-stack/masonry artifacts, coherent roads.
  - Weaknesses: too map-like / bird's-eye; weak illustrated notebook aesthetic.

- `grove-cleanroom-subagent-maponly-b.png`
  - Inputs: Grove map crop only.
  - Strengths: same artifact fixes as A; coherent plate.
  - Weaknesses: too survey-map-like and simplified; not enough 3/4 playable hub
    feel.

- `grove-cleanroom-subagent-style-ref-a.png`
  - Inputs: Grove map crop + full `illustrated-parish-notebook.png`.
  - Strengths: stronger style and facade readability.
  - Weaknesses: copied some composition habits from the full reference; church
    became too prominent.

- `grove-cleanroom-subagent-style-ref-b.png`
  - Inputs: Grove map crop + full `illustrated-parish-notebook.png`, after
    stronger "style not composition" wording.
  - Strengths: good illustrated style and no artifact regressions.
  - Weaknesses: still too high/zoomed-out; church remains too prominent.

- `grove-cleanroom-subagent-style-swatch-a.png`
  - Inputs: Grove map crop + small style crops.
  - Strengths: better style than map-only, fewer composition-copying issues than
    full reference.
  - Weaknesses: copied some swatch props; church still fairly prominent; pitch
    slightly high.

- `grove-cleanroom-subagent-style-swatch-b.png`
  - Inputs: Grove map crop + small style crops, after prop-copying and
    secondary-landmark wording.
  - Strengths: no visible smoke, no invented water, no freestanding
    roof-stack/masonry artifacts, less prop copying, coherent roads.
  - Weaknesses: still too steep/top-down; secondary church context remains too
    visually strong.

- `grove-cleanroom-subagent-style-swatch-c.png`
  - Inputs: Grove map crop + small style crops, after stronger camera/framing
    language.
  - Strengths: Grove is visually dominant; no smoke, invented water, labels,
    UI, freestanding masonry artifacts, or obvious route breakage.
  - Weaknesses: still reads too aerial; church remains a strong secondary anchor.

- `grove-cleanroom-subagent-style-swatch-d.png`
  - Inputs: Grove map crop + small style crops, same prompt as C.
  - Strengths: clean artifact control and no major landmark copying.
  - Weaknesses: still a survey-like high camera; fields/roads take too much
    frame weight.

- `grove-cleanroom-subagent-full-style-ref-c.png`
  - Inputs: Grove map crop + full `illustrated-parish-notebook.png`.
  - Strengths: good illustrated style, Grove readable, route continuity mostly
    coherent.
  - Weaknesses: camera too aerial; church became a major focal element.

- `grove-cleanroom-subagent-full-style-ref-d.png`
  - Inputs: Grove map crop + full `illustrated-parish-notebook.png`, with
    stronger target-site framing.
  - Strengths: Grove central, church edge-biased, no major artifact regressions.
  - Weaknesses: still higher than ideal.

- `grove-cleanroom-subagent-full-style-ref-e-target-extraction.png`
  - Inputs: Grove map crop + full style reference after allowing distant
    landmarks to be omitted.
  - Strengths: first strong target-site plate; no church; Grove dominant; good
    connectivity.
  - Weaknesses: camera still slightly high.

- `grove-cleanroom-subagent-full-style-ref-f-target-extraction.png`
  - Inputs: same as E.
  - Strengths: no church dominance, Grove central, artifact control good.
  - Weaknesses: still includes more field/road context than ideal and reads a
    little high.

- `grove-cleanroom-subagent-full-style-ref-g-sprite-calibrated.png`
  - Inputs: Grove map crop + full style reference after sprite/door/facade
    calibration.
  - Strengths: Grove dominant, no church, gates/roads usable.
  - Weaknesses: roofs still dominate slightly; thresholds/outbuildings are a bit
    small for sprites.

- `grove-cleanroom-subagent-full-style-ref-h-sprite-calibrated.png`
  - Inputs: same as G.
  - Strengths: clean, readable Grove plate; doors and facades pass with caveats;
    no smoke, invented water, UI, labels, or route breakage.
  - Weaknesses: still mildly higher than the ideal low 3/4 camera.

- `grove-map-target-site-crop.png`
  - Inputs: derived from the supplied wide Grove map crop with
    `ffmpeg crop=1200:820:560:220`.
  - Purpose: remove arbitrary district context while keeping Grove,
    orchard/garden, local roads, field boundaries, and exits.

- `grove-cleanroom-subagent-target-map-crop-a.png`
  - Inputs: `grove-map-target-site-crop.png` + full
    `illustrated-parish-notebook.png`.
  - Strengths: practical pass on camera, sprite-scale doors, Grove dominance,
    map context, artifact control, and roads/gates.
  - Weaknesses: still a little high; small cart/barrel props echo the reference.

- `grove-cleanroom-subagent-target-map-crop-b.png`
  - Inputs: same as A.
  - Strengths: strongest cleanroom result so far; exact 16:9, Grove dominant,
    readable facades/doors, no church/context drift, no smoke/water/UI/label
    artifacts, roads/gates continuous enough for a plate test.
  - Weaknesses: mildly high compared with the ideal, but usable.

## Recommended Prompt Direction

The best current direction is the generic
`portable-background-plate-one-shot-template.md` with a target-site map crop and
no per-location hint notes:

- target-site crop is the layout reference,
- optional reproducible map-reader notes or control images can provide layout
  support only if generated uniformly for every location,
- full illustrated notebook render provides style/camera cues,
- map extent is explicitly not the output frame,
- source-map north/top should remain final-image top,
- distant landmarks can be omitted if they would force a regional overview,
- road/boundary interpretation must come from the map, generic legend, or a
  reproducible map-reader/control pipeline, not hand-authored place notes,
- sprite/door/facade language calibrates playable scale,
- visible smoke is banned because smoke should be a runtime/composited layer,
- route-graph language has been removed; layout/connectivity is map-derived,
- freestanding roof-stack/masonry artifacts are discouraged with one concise
  physical rule rather than repeated bad-object naming.

## Remaining Problem

The latest recipe is usable but still tends mildly high. Text prompting alone
does not fully overcome the model's learned "map-to-aerial-illustration" habit.

Likely next iterations:

- generate or hand-author an explicit camera/style sheet with the desired
  low-oblique building proportions,
- use a preprocessed layout mask or target-site crop generated from GIS/map
  scale before image generation,
- test whether lower-fidelity or more game-board-like references reduce the
  high-aerial bias,
- keep the target-site crop step in the workflow for batch production.
