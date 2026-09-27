# Pipeline Experiments

Clean-context image-generation experiments for the historic-map-to-background
plate pipeline. Each experiment has:

- a generated PNG,
- the exact prompt used,
- a short report.

No experiment used hand-authored per-location road, wall, river, building, or
landmark hints.

## Binary Archive

Wave 3 moved this directory's 474 PNGs to the verified, content-addressed
external archive
`graphics-v2-pipeline-experiments-b467cae6-20260826T020635Z-manifest-078b3883c20c`.
[`archive-index.tsv`](archive-index.tsv) records every original path, byte
count, SHA-256, Git blob ID, provenance class, and licensing obligation. The
image names below are archive-relative identifiers; the prompt, report, and
audit sidecars remain tracked here.

New PNGs under this directory are ignored working output. Archive a retained
run and update the index, or deliberately promote a reviewed clean-checkout
input to [`../map-sources/`](../map-sources/README.md) or
[`../authorities/`](../authorities/README.md). Do not force-add experiment PNGs.

## Outputs

| ID         | Image                                                                                                                                                     | Prompt                                                | Report                                                | Result                                                                                                                                                               |
| ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------- | ----------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A          | `idea-a-map-only.png`                                                                                                                                     | `idea-a-map-only.prompt.md`                           | `idea-a-map-only.report.md`                           | Best source fidelity; pass with caveat                                                                                                                               |
| B          | `idea-b-oblique-warp.png`                                                                                                                                 | `idea-b-oblique-warp.prompt.md`                       | `idea-b-oblique-warp.report.md`                       | Fail                                                                                                                                                                 |
| C          | `idea-c-semantic-mask.png`                                                                                                                                | `idea-c-semantic-mask.prompt.md`                      | `idea-c-semantic-mask.report.md`                      | Pass                                                                                                                                                                 |
| D          | `idea-d-extruded-blockout.png`                                                                                                                            | `idea-d-extruded-blockout.prompt.md`                  | `idea-d-extruded-blockout.report.md`                  | Pass with caveat                                                                                                                                                     |
| E          | `idea-e-oblique-ink-warp.png`                                                                                                                             | `idea-e-oblique-ink-warp.prompt.md`                   | `idea-e-oblique-ink-warp.report.md`                   | Fail                                                                                                                                                                 |
| F          | `idea-f-linework-control.png`                                                                                                                             | `idea-f-linework-control.prompt.md`                   | `idea-f-linework-control.report.md`                   | Control-path pass, lower source fidelity                                                                                                                             |
| G          | `idea-g-raw-map-control-02.png`                                                                                                                           | `idea-g-raw-map-control-02.prompt.md`                 | `idea-g-raw-map-control-02.report.md`                 | Fail                                                                                                                                                                 |
| H          | `idea-h-grove-raw-style-crops.png`, `idea-h-beechwood-raw-style-crops.png`                                                                                | `idea-h-*.prompt.md`                                  | `idea-h-*.report.md`                                  | Safer than full style scene, but still prop/style leakage risk                                                                                                       |
| I          | `idea-i-grove-strict-map-fidelity.png`, `idea-i-beechwood-strict-map-fidelity.png`                                                                        | `idea-i-*.prompt.md`                                  | `idea-i-*.report.md`                                  | Cleaner semantics, but over-compressed footprints                                                                                                                    |
| J          | `idea-j-grove-clean-style-swatches.png`, `idea-j-beechwood-clean-style-swatches.png`                                                                      | `idea-j-*.prompt.md`                                  | `idea-j-*.report.md`                                  | Current paired pass                                                                                                                                                  |
| K          | `idea-k-grove-map-reader-guided.png`, `idea-k-beechwood-map-reader-guided.png`                                                                            | `idea-k-*.prompt.md`                                  | `idea-k-*.report.md`                                  | Current best for building interpretation                                                                                                                             |
| L          | `idea-l-*-topdown-cleaned.png`, `idea-l-*-two-step-isomorphic.png`                                                                                        | `idea-l-*.prompt.md`                                  | `idea-l-*.report.md`                                  | Promising; stronger central topology, needs native 16:9 discipline                                                                                                   |
| M          | `idea-m-*-admin-topdown-cleaned.png`, `idea-m-*-admin-two-step-isomorphic.png`                                                                            | `idea-m-*.prompt.md`                                  | `idea-m-*.report.md`                                  | Current best two-step path; suppresses non-physical admin boundaries                                                                                                 |
| N          | `idea-n-*-notebook-style-isomorphic.png`                                                                                                                  | `idea-n-*.prompt.md`                                  | `idea-n-*.report.md`                                  | Restores rough notebook art style, camera still high                                                                                                                 |
| O          | `idea-o-grove-lower-camera-notebook-style.png`                                                                                                            | `idea-o-grove-lower-camera-notebook-style.prompt.md`  | `idea-o-grove-lower-camera-notebook-style.report.md`  | Grove camera improvement pass                                                                                                                                        |
| P          | `idea-p-*-lower-angle-notebook-style.png`                                                                                                                 | `idea-p-*.prompt.md`                                  | `idea-p-*.report.md`                                  | Current best style/camera candidate                                                                                                                                  |
| Q          | `idea-q-*-camera-refinement-notebook-style.png`                                                                                                           | `idea-q-*-camera-refinement-notebook-style.prompt.md` | `idea-q-*-camera-refinement-notebook-style.report.md` | Current best overall candidate                                                                                                                                       |
| R          | `idea-r-*-close-crop-notebook-style.png`                                                                                                                  | `idea-r-*.prompt.md`                                  | `idea-r-*.report.md`                                  | Close playable crop; best scale/detail pass                                                                                                                          |
| S          | `idea-s-*-lower-rough-close-crop-notebook-style.png`                                                                                                      | `idea-s-*.prompt.md`                                  | `idea-s-*.report.md`                                  | Marginal roughness refinement                                                                                                                                        |
| T/U        | `idea-t-grove-*`, `idea-u-*-tight-thatched-door-clean-style.png`                                                                                          | `idea-t-*`, `idea-u-*`                                | `idea-t-*`, `idea-u-*`                                | Best tight art/camera/material branch; topology control caveat                                                                                                       |
| V/W/X      | `idea-v-*`, `idea-w-*`, `idea-x-beechwood-compound-focused-low-camera.png`                                                                                | `idea-v-*`, `idea-w-*`, `idea-x-*`                    | `idea-v-*`, `idea-w-*`, `idea-x-*`                    | Best Beechwood topology-control + notebook-scale branch                                                                                                              |
| X/Y        | `idea-x-beechwood-*`, `idea-y-grove-cluster-focused-low-camera.png`                                                                                       | `idea-x-*`, `idea-y-*`                                | `idea-x-*`, `idea-y-*`                                | Current paired crop-scale evidence                                                                                                                                   |
| Z          | `idea-z-beechwood-x-door-roughness-refine.png`, `idea-z-grove-y-lower-camera-refine.png`                                                                  | `idea-z-*`                                            | `idea-z-*`                                            | Current best repaired pair                                                                                                                                           |
| AA         | `idea-aa-*-direct-control-low-camera.png`                                                                                                                 | `idea-aa-*`                                           | `idea-aa-*`                                           | Direct-control pass, style/camera regression                                                                                                                         |
| AC         | `idea-ac-*-direct-control-clean-style.png`                                                                                                                | `idea-ac-*`                                           | `idea-ac-*`                                           | Best direct-control branch so far                                                                                                                                    |
| AD         | `idea-ad-*-ac-bounded-notebook-refine.png`                                                                                                                | `idea-ad-*`                                           | `idea-ad-*`                                           | Best visual repair pair from AC                                                                                                                                      |
| AE         | `idea-ae-*-core-direct-clean-style.png`                                                                                                                   | `idea-ae-*`                                           | `idea-ae-*`                                           | Best direct crop-scale branch; chimney-nub caveat                                                                                                                    |
| AF         | `idea-af-*-ae-roof-nub-cleanup.png`                                                                                                                       | `idea-af-*`                                           | `idea-af-*`                                           | Roof-nub cleanup; Beechwood fails foreground door audit                                                                                                              |
| AG         | `idea-ag-beechwood-af-door-threshold-repair.png`                                                                                                          | `idea-ag-*`                                           | `idea-ag-*`                                           | Beechwood foreground door repair; bounded edit, not recipe evidence                                                                                                  |
| AH         | `idea-ah-kilteevan-third-topology-direct.png`                                                                                                             | `idea-ah-*`                                           | `idea-ah-*`                                           | Third-topology direct test; good topology signal, boundary/chimney failures                                                                                          |
| AI         | `idea-ai-*-kilteevan-*.png`                                                                                                                               | `idea-ai-*`                                           | `idea-ai-*`                                           | Roof/door/style improved; admin-boundary no-trace still fails                                                                                                        |
| AK         | `idea-ak-beechwood-door-audit-repair.png`                                                                                                                 | `idea-ak-*`                                           | `idea-ak-*`                                           | Corrects missed lower-right foreground cottage door; bounded repair                                                                                                  |
| AL         | `idea-al-beechwood-thatched-no-chimney.png`                                                                                                               | `idea-al-*`                                           | `idea-al-*`                                           | Thatched/no-chimney material variant from repaired AK plate                                                                                                          |
| AM         | `idea-am-kilteevan-aj2-cleaned-control-direct.png`                                                                                                        | `idea-am-*`                                           | `idea-am-*`                                           | Cleaned no-admin control improves deleted-diagonal behavior; walls still too emphatic                                                                                |
| AN         | `idea-an-kilteevan-boundary-hierarchy-direct.png`                                                                                                         | `idea-an-*`                                           | `idea-an-*`                                           | Better notebook style/facades; field boundaries still too wall-like                                                                                                  |
| AO         | `idea-ao-kilteevan-open-fields-direct.png`                                                                                                                | `idea-ao-*`                                           | `idea-ao-*`                                           | Best open-field softness/topology signal; chimney artifacts fail clean target                                                                                        |
| AP/AP2/AP3 | `idea-ap*-kilteevan-*.png`                                                                                                                                | `idea-ap*`                                            | `idea-ap*`                                            | Bounded cleanup attempts; AP fails, AP2 improves, AP3 is best visual cleanup with repaint caveat                                                                     |
| AQ         | `idea-aq-kilteevan-direct-open-fields-no-chimneys.png`                                                                                                    | `idea-aq-*`                                           | `idea-aq-*`                                           | Stronger notebook/no-chimney fresh render; weaker control fidelity                                                                                                   |
| AR         | `idea-ar-kilteevan-tight-control-no-scenic-crossroads.png`                                                                                                | `idea-ar-*`                                           | `idea-ar-*`                                           | Tighter crop fresh render; better than AQ, roads still scenic-centered                                                                                               |
| AS/AS2     | `idea-as*-kilteevan-playable-roadcue-*`                                                                                                                   | n/a                                                   | `idea-as*-*control-report.md`                         | Failed deterministic road-cue masks; too noisy for imagegen authority                                                                                                |
| AT/AU      | `idea-at-kilteevan-tight-*.png`, `idea-au-kilteevan-at2-wall-door-repair.png`                                                                             | `idea-at-*`, `idea-au-*`                              | `idea-at-*`, `idea-au-*`                              | Tight two-step improves camera/doors/no-chimneys; AU repair best visual target, not recipe proof                                                                     |
| AV         | `idea-av-kilteevan-symbolic-*.png`                                                                                                                        | `idea-av-*`                                           | `idea-av-*`                                           | Symbolic top-down retry; visually strong but not better than AU/AT                                                                                                   |
| AW/AX      | `idea-aw-kilteevan-literal-control-isomorphic.png`, `idea-ax-kilteevan-door-repair.png`                                                                   | `idea-aw-*`, `idea-ax-*`                              | `idea-aw-*`, `idea-ax-*`                              | Literal control gets strong style/doors but poor topology; AX is door repair only                                                                                    |
| AY/AZ      | `idea-ay-kilteevan-au-notebook-style-refine.png`, `idea-az-kilteevan-ay-low-camera-refine.png`                                                            | `idea-ay-*`, `idea-az-*`                              | `idea-ay-*`, `idea-az-*`                              | Bounded AU visual refinements; AZ best visual target, not recipe proof                                                                                               |
| BA/BB      | `idea-ba-kilteevan-fresh-map-control-notebook.png`, `idea-bb-kilteevan-ba-boundary-soften.png`                                                            | `idea-ba-*`, `idea-bb-*`                              | `idea-ba-*`, `idea-bb-*`                              | Best fresh no-prior-render notebook recipe attempt plus boundary-softened repair                                                                                     |
| BC         | `idea-bc-kilteevan-boundary-material-*.png`                                                                                                               | `idea-bc-*`                                           | `idea-bc-*`                                           | Boundary-material control test; negative result, outlines still become walls and roof nub returns                                                                    |
| BD         | `idea-bd-kilteevan-soft-planting-*.png`                                                                                                                   | `idea-bd-*`                                           | `idea-bd-*`                                           | Soft-planting edge suppression; doors/roofs improve, roads/gardens still over-regularize                                                                             |
| BE         | `idea-be-kilteevan-raw-map-notebook-no-topdown.png`                                                                                                       | `idea-be-*`                                           | `idea-be-*`                                           | Raw/cleaned-map-only retry; still scenic-crossroads drift                                                                                                            |
| BF/BG/BH   | `idea-bf-grove-a-topology-notebook-refine.png`, `idea-bg-grove-a-structure-preserving-notebook-refine.png`, `idea-bh-grove-bg-upper-structure-repair.png` | `idea-bf-*`, `idea-bg-*`, `idea-bh-*`                 | `idea-bf-*`, `idea-bg-*`, `idea-bh-*`                 | Best current visual target branch: Cycle A topology repaint plus local repair; edit evidence only                                                                    |
| BJ/BK/BL   | `idea-bj-beechwood-q-notebook-repaint.png`                                                                                                                | `idea-bj-*`, `idea-bk-*`, `idea-bl-*`                 | `idea-bj-*`, `idea-bk-*`, `idea-bl-*`                 | Beechwood Q/M bounded repaint; BJ improves notebook style while preserving topology, BL is the preferred queued soft-garden pass                                     |
| BM         | `idea-bm-e1-*` through `idea-bm-e6-*`                                                                                                                     | `idea-bm-e*.prompt.md`                                | `idea-bm-e*.report.md`                                | Isomorphic transform calibration; 55% crop + `y_squash ~= 0.40` fixes camera/zoom most, path/wall/garden semantics remain                                            |
| BN         | `idea-bn-e1-*`, `idea-bn-e2-*`                                                                                                                            | `idea-bn-e*.prompt.md`                                | `idea-bn-e*.report.md`                                | North-extended incremental low-camera test; E2 proves the 10-12 degree camera target, but boundary/garden semantics harden                                           |
| BO         | `idea-bo-e1-*`, `idea-bo-e2-*`                                                                                                                            | `idea-bo*.prompt.md`                                  | `idea-bo*.report.md`                                  | Final-step orthographic rectification from BN E2; E2 is best, E1 overbuilds fences/walls                                                                             |
| BP         | `idea-bp-e1-*`, `idea-bp-e2-*`                                                                                                                            | `idea-bp-e*.prompt.md`                                | `idea-bp-e*.report.md`                                | Art-last repaint with hard isomorphic grid check; E2 is preferred style target, E1 is stricter geometry baseline                                                     |
| BQ         | `idea-bq-*`                                                                                                                                               | `idea-bq-e1-*.prompt.md`                              | `idea-bq-*.report.md`                                 | Adds constant-scale marker audit; E1 partially fixes far-tree miniaturization but hardens vegetation/gardens                                                         |
| BR         | `idea-br-*`                                                                                                                                               | `idea-br-e1-*.prompt.md`                              | `idea-br-*.report.md`                                 | Close Beechwood concept-art branch; relaxes strict isomorphic scale, zooms closer, raises camera, restores doors                                                     |
| BS         | `idea-bs-e1-*`, `idea-bs-e2-*`                                                                                                                            | `idea-bs-e*.prompt.md`                                | `idea-bs-e*.report.md`                                | Door-height calibration from BR; E2 zooms out 20% more while keeping fitted doors                                                                                    |
| BT         | `idea-bt-e1-*` through `idea-bt-e3-*`                                                                                                                     | `idea-bt-e*.prompt.md`                                | `idea-bt-e*.report.md`                                | Weathering/clutter prompt matrix for BS E2; E2 best single image, E2+E3 hybrid recommended                                                                           |
| BU         | `idea-bu-e1-*`, `idea-bu-e2-*`                                                                                                                            | `idea-bu-e*.prompt.md`                                | `idea-bu-e*.report.md`                                | Concept-realism convergence pass from BT; E2 is the accepted visual target                                                                                           |
| BV         | `idea-bv-e1-*`, `idea-bv-e2-*`                                                                                                                            | `idea-bv-e*.prompt.md`                                | `idea-bv-e*.report.md`                                | Grove reproducible pipeline validation; E1 proves transfer, E2 is preferred visual output                                                                            |
| BW         | `idea-bw-e1-*` through `idea-bw-e4-*`                                                                                                                     | `idea-bw-e*.prompt.md`                                | `idea-bw-e*.report.md`                                | Grove dry-stone/regional-boundary pass; E4 shows real-wall reference alone is insufficient                                                                           |
| BX         | `idea-bx-e1-*`, `idea-bx-e2-*`                                                                                                                            | `idea-bx-e*.prompt.md`                                | `idea-bx-e*.report.md`                                | Murphy's Farm BU-style pipeline pass; E1 is direct recipe evidence, E2 is preferred bounded roof/boundary fix                                                        |
| BY         | `idea-by-e1*`, `idea-by-e2*`, `idea-by-e3*`                                                                                                               | `idea-by-*.prompt.md`                                 | `idea-by-*.report.md`                                 | Murphy's Farm geometry-match and style-last recovery; E2d is the hard geometry target, E2f is the geometry render, and E3c is the preferred accuracy+style candidate |
| BZ         | `idea-bz-grove-subagent-*`                                                                                                                                | `idea-bz-grove-subagent-bu-style.prompt.md`           | `idea-bz-grove-subagent-bu-style.*report.md`          | First fully subagent-gated Grove proof; PASS WITH CAVEATS, geometry/doors/style pass but ambiguous boundaries over-promote into continuous stone walls               |

See `beechwood-church-leak-analysis.md` for the likely cause of Cycle G's
unsupported church/churchyard: semantic leakage from the full-scene style
reference, amplified by ambiguous estate/building/enclosure geometry in the raw
map crop.

## Current Signal

Cycle A is the most accurate source-map read so far. The next direction is raw
map as the primary layout evidence plus cropped style references, with linework
and semantic controls used as soft secondary aids or post-generation checks.
Raw oblique warps and full-scene style references are risky: they encourage
invented water/bridges, chapel/cemetery motifs, or copied map artifacts. Heavy
building blockouts should wait until building extraction is more reliable.

Cycle G repeats the Cycle A method on a second raw map crop. It failed, which
suggests Cycle A's success came from the raw-map-first method plus a cooperative
target crop, not from a universally reliable one-shot recipe.

Cycle J is the current raw-map/style-swatch baseline because it passed the same
prompt/reference set on Grove and Beechwood. The key change was replacing the
full illustrated style scene with small style/material swatches that do not
contain churches, graveyards, bridges, rivers, labels, UI, or obvious props. See
`../one-shot-background-plate-candidate-cycle-j.md`.

Cycle K adds a reproducible map-reader stage before rendering. The map-reader
notes are generated from each crop with the same clean-context rubric and saved
as auditable artifacts. This improved building interpretation on both control
crops while preserving Cycle J's semantic guardrails. See
`../map-reader-stage-template.md` and
`../one-shot-background-plate-candidate-cycle-k.md`.

Cycle L tests a two-step image-generation path: first a top-down cleaned control
plate, then an isomorphic conversion. The central map topology looks stronger
than Cycle K, especially for roads, enclosures, tree masses, and building
separation. The remaining weakness is native 16:9 discipline: Beechwood's final
16:9 output required synthetic horizontal edge extension. See
`../two-step-topdown-isomorphic-cycle-l.md`.

Cycle M keeps the two-step path and adds a generic administrative/survey
boundary ignore class for unsupported dotted, pecked, dashed, and dot-chain
linework. It also tests a wider Grove source crop. Beechwood's prominent
eastern dotted boundary no longer becomes a fake hedge or wall, and both final
M2 plates were saved as returned without synthetic edge extension. Exact native
16:9 is still unresolved. See `../two-step-admin-boundary-cycle-m.md`.

Cycle N adds the original illustrated parish notebook scene as a style-only
reference. It improves the rough hand-inked watercolor look but keeps a
survey-like camera. Cycle P is the current best final-render candidate: it keeps
Cycle M's topology/control path while using a lower 3/4 orthographic camera
block, stronger facade-height targets, and the notebook style reference. Grove
is the strongest camera result; Beechwood generalizes the style and topology but
still reads somewhat high in the planted enclosure. See
`../lower-angle-notebook-style-cycle-p.md`.

Cycle Q refines the Cycle P plates by using the successful prior plate as the
style/topology target, the Cycle M top-down control as the topology authority,
and a deterministic oblique warp only as a camera-pitch cue. It is the current
best overall candidate for the notebook-style + Cycle M accuracy objective.
The rough connected-component blockouts generated during Cycle Q are not yet
recommended; they over-detect texture as buildings. See
`../camera-refinement-cycle-q.md`.

Cycle R keeps the Cycle Q evidence stack but changes the composition: same
pixel frame, much smaller playable world area. Both Grove and Beechwood spend
more pixels on the central building cluster, immediate yard/courtyard, nearby
garden/enclosure edge, gates, walls, and road exits. This is the strongest
scale/detail pass so far and confirms that the original notebook sample's
richness depends partly on crop scale, not only on style language. See
`../close-crop-notebook-style-cycle-r.md`.

Cycle S directly refines Cycle R with stronger lower-camera and roughness
language. It improves surface texture only marginally and mostly preserves
Cycle R's polished isometric regularity. Treat it as a useful negative signal:
the next attempt needs a stronger isolated notebook style/camera reference, not
just more forceful wording against the generated R plate. See
`../lower-rough-close-crop-cycle-s.md`.

Cycles T/U zoom in further and introduce cleaned low-camera door/threshold
style references, including a single-house thatch/no-chimney crop. Grove U is
the best art/material/camera result so far. Beechwood U shows the same style
direction can generalize, but also exposes the key remaining pipeline risk:
prompt-only tight cropping can dissolve complex connected footprints into a
looser farmstead cluster. See `../tight-low-camera-thatched-cycle-t-u.md`.

Cycles V/W/X answer the Beechwood topology caveat with a tighter topology crop.
Cycle V preserves the connected compound but still reads like a high controlled
plate; Cycle W fixes a weak edge-building doorway and softens the style; Cycle X
uses a smaller compound-focused crop and is the best Beechwood notebook-scale
candidate so far. The main lesson is to choose the desired playable scale before
rendering and let distant map context crop off-frame rather than forcing the
whole garden into one survey view. See `../tight-control-cycle-v-w-x.md`.

Cycle Y repeats the Cycle X crop-scale method on Grove using a Grove-specific
tight control crop but the same generic prompt structure. It preserves Grove's
separate yard buildings instead of copying Beechwood's connected-compound
layout, which is the first useful paired signal that the method can generalize
across distinct local topologies. See
`../grove-beechwood-compound-crop-cycle-x-y.md`.

Cycle Z performs one conservative repair/refinement pass on the X/Y pair.
Beechwood Z keeps the connected compound and clarifies the ambiguous side
doorway; Grove Z keeps the separate yard buildings and nudges the camera/style
closer to the Beechwood/notebook target. Treat Z as the current best visual
reference pair, but still not a full production recipe until the method passes
additional unrelated topology crops. See `../paired-repair-cycle-z.md`.

Cycle AA tests a cleaner production-shaped route: render directly from local
topology control, oblique pitch cue, source map, original notebook sample, and
style crops, without using prior rendered plates. It preserves the tested
topologies but regresses toward clean/high survey-board imagery. See
`../direct-control-cycle-aa.md`.

Cycle AC keeps AA's direct-control route but replaces the leaky slate style crop
with the cleaned single-building slate crop and pushes harder on close playable
crop scale, lower facades, notebook watercolor texture, and anti-regularity.
It is the strongest production-shaped direct-control branch so far: Beechwood's
connected compound and Grove's separate-building yard both survive without prior
rendered plates. The remaining weakness is visual regularity in gardens, walls,
and roof planes compared with the original notebook sample. See
`../direct-control-clean-style-cycle-ac.md`.

Cycle AD performs one bounded style/camera repair pass on AC. It keeps the
tested topology and improves rough ink, watercolor grain, mud, facade
readability, and irregular field/garden texture. It is the best current visual
pair, but it is diagnostic rather than a one-shot candidate because it uses AC
as a previous rendered edit target. The remaining gap is lower human-scale
camera/facade feel. See `../ac-bounded-notebook-refine-cycle-ad.md`.

Cycle AE attacks the same camera/facade gap from the direct-control side by
cropping the topology controls tighter around the core local buildings and
yards. The hypothesis is that smaller ground coverage will naturally spend more
pixels on facades, thresholds, mud, walls, and hand-painted texture. The
hypothesis held: AE is the strongest direct-control camera/scale branch so far,
but both plates need a tiny roof-nub/chimney cleanup. See
`../core-crop-direct-cycle-ae.md`.

Cycle AF is that tiny roof-nub cleanup pass on AE. It removed the chimney-like
roof nubs while preserving the AE topology/style/crop, but Beechwood AF fails a
stricter door/threshold audit: the lower-right foreground thatched cottage has
no readable entrance. AE remains the direct-control recipe evidence because it
does not use a previous rendered plate. See
`../ae-roof-nub-cleanup-cycle-af.md`.

Cycle AG repairs the Beechwood AF doorway failure. The lower-right foreground
thatched cottage now has a readable dark doorway and threshold while preserving
the Beechwood topology, crop, roads, walls, gates, garden plots, and notebook
style. Treat Beechwood AG plus Grove AF as the current cleaned visual reference
pair, and treat AE as the direct-control recipe evidence. See
`../door-threshold-repair-cycle-ag.md`.

Cycle AH adds a third topology test using a data-derived NLS z17 crop around a
stored world coordinate, a clean-context map-reader note, and the direct
map/control prompt family. It preserves the broad village topology surprisingly
well, but fails the clean-plate bar by converting at least one likely
administrative/survey line into stone walling and adding a chimney-like stack.
Treat it as useful generalization evidence and a boundary/chimney negative
signal, not a visual target. See `../third-topology-kilteevan-cycle-ah.md`.

Cycle AI reruns the same third crop with stronger no-trace boundary language
and zero-tolerance roof-protrusion language. AI-A keeps the full notebook scene
style reference; AI-B uses only cleaned style crops/material swatches. Both fix
the chimney/roof problem and preserve the major topology, but both still turn
too much ambiguous linework into physical stone walls. The next step should be
an upstream cleaned physical-linework control that suppresses admin/survey
boundaries before imagegen. See `../boundary-roof-retry-cycle-ai.md`.

Cycle AK fixes a concrete visual-audit miss: the lower-right foreground cottage
in the Beechwood cleaned plate still lacked a readable doorway. AK adds a dark
door and threshold while preserving the repaired layout. Cycle AL then tests a
thatched/no-chimney roof-material variant from AK. Both are bounded visual
edits, not fresh direct-control recipe evidence.

Cycle AM uses AJ2's cleaned no-admin map crop as a physical-linework control
while keeping the original crop as layout authority. It improves the specific
failure from AH/AI: the bold deleted diagonal dot-chain is not plainly restored
as one continuous wall or hedge. The plate still over-materializes many thin
plot lines as stone walls, so the next iteration should focus on a boundary
material hierarchy rather than only dot-chain deletion. See
`../cleaned-boundary-control-cycle-aj-am.md`.

Cycle AN adds that hierarchy and reintroduces the full illustrated notebook
sample as style-only. It improves the original-notebook feel and building
readability without obvious semantic leakage, while preserving the major
Kilteevan topology. The remaining failure is still wall restraint: many field
and enclosure edges become continuous stone walls. See
`../boundary-hierarchy-cycle-an.md`.

Cycle AO tightens AN's open-field rule. It is the best direct-control signal so
far for avoiding a stone-wall network while preserving the Kilteevan road,
building, garden, and tree topology. It is not a clean visual target because it
reintroduces small chimneys/roof-stack artifacts. Treat AO as recipe evidence
for boundary hierarchy, with any roof cleanup kept as a separate bounded edit.
See `../open-field-boundary-cycle-ao.md`.

Cycle AP/AP2/AP3 are those bounded roof/stub cleanup attempts. AP still left
obvious roof artifacts; AP2 removed the lower-left chimney and preserved major
doors/topology, but still had a questionable upper-compound roof mark and some
global repaint softness. AP3 removes that remaining obvious upper-compound roof
nub and passes the inspected door/threshold crops, but it is still a
whole-plate edit rather than a strictly pixel-local repair. Treat AP3 as visual
target evidence with caveats, not as one-shot recipe proof. See
`../ao-roof-stub-cleanup-cycle-ap.md`.

Cycle AQ is a fresh direct-control retry that keeps AO's evidence stack but adds
stronger conflict rules and absolute no-chimney roof language. It improves
notebook feel and mostly avoids roof protrusions without a cleanup edit, but it
loses control fidelity by inventing a more composed rural crossroads and adding
more wall/boundary fragments. Treat AQ as useful roof/style prompt evidence,
not a replacement for AO/AP3. See
`../direct-open-fields-no-chimneys-cycle-aq.md`.

Cycle AR keeps AQ's roof/conflict language but uses a tighter playable
map/control crop and matching oblique cue. It is the better fresh one-shot
direction after AQ: crop scale, notebook feel, open fields, and no-chimney
discipline improve. It still regularizes roads into a centered scenic
Y/crossroads, so prompt/crop control alone is not enough. The next attempt needs
a stronger deterministic road/topology cue. See
`../tight-control-no-scenic-crossroads-cycle-ar.md`.

Cycle AS/AS2 tries to make that deterministic road/topology cue with a generic
pale-corridor detector in `prototype_map_controls.py`. The result is too noisy
and should not be used as an imagegen authority: it highlights tree/symbol
clusters and generic pale gaps along with roads. See
`../road-topology-cue-cycle-as.md`.

Cycle AT returns to the earlier top-down-cleaned-to-isomorphic path using the
tighter AR playable crop. AT1 is a clean top-down control but still turns a
cleaned admin/erasure scar into a physical-looking boundary. AT2 improves the
low 3/4 camera, doors, road continuity, notebook texture, and no-chimney
discipline, but it over-materializes continuous stone/wall boundaries around
roads, fields, and the garden. Cycle AU is a bounded repair from AT2 that
softens those walls and clarifies shed doors; it is the best visual target from
this crop, but not one-shot recipe evidence. See
`../tight-two-step-wall-door-cycle-at-au.md`.

Cycle AV tries to solve AT's wall problem by making the top-down control more
symbolic and minimal-boundary. AV1 reduces some hard walling but expands and
regularizes the scene into a prettier plan; AV2 is attractive but still trends
toward a scenic crossroads, continuous garden/compound outlines, and marginal
small-shed doors. Treat AV as negative evidence for freehand generated
top-down controls. The next recipe needs a more deterministic/literal
paint-by-numbers control that preserves crop extent and feature uncertainty
before imagegen. See `../symbolic-topdown-control-cycle-av.md`.

Cycle AW tests that deterministic/literal paint control. It is a gorgeous
negative result: door, roof, thatch/slate, and notebook texture are strong, but
the final plate still regularizes into a picturesque walled crossroads with
worse topology than AT/AU. Cycle AX repairs the foreground-house door on that
branch, but the branch remains topology-poor. See
`../literal-paint-control-cycle-aw.md`.

Cycle AY/AZ returns to AU, the stronger topology-preserving visual target, and
uses bounded edits to recover more notebook texture, facade weight, and
doors-on-openings clarity. AZ is now the best visual target for this tight
Kilteevan crop, but it is not one-shot recipe evidence because it edits prior
rendered plates and slightly emphasizes garden fencing. See
`../au-notebook-refinement-cycle-ay-az.md`.

Cycle BA asks whether the AZ/notebook direction can be reached without using a
previous isomorphic render. It is the best fresh no-prior-render attempt so far:
topology is much better than AW/AX and the notebook style is strong. The same
failure remains in a sharper form: garden/internal control lines become too
wall-like and the road junction still becomes a little composed. Cycle BB shows
a bounded de-wall/de-diagram repair can soften BA while preserving the scene,
but it also washes out some crisp ink/facade density. See
`../fresh-map-control-notebook-cycle-ba-bb.md`.

Cycle BC adds a deterministic boundary-material control to make garden/orchard
texture read as soft planting rather than wallable edges. It is a useful
negative result: the final render is fresh and notebook-like, but the visible
outlines in the control still become hard garden boundaries, vegetation becomes
too regular, and a main-roof chimney/nub returns. The next control should blur
or suppress garden perimeters rather than merely tint dense planting. See
`../boundary-material-control-cycle-bc.md`.

Cycle BJ returns to the strongest Beechwood Q/M evidence stack and uses a
bounded repaint rather than a fresh render. It preserves the connected
compound, diagonal road, attached garden, lower building group, tree mass, and
open-field layout while moving the art toward heavier sepia ink, mottled
watercolor, better facade/door readability, and no-chimney roof discipline.
The remaining gap is garden regularity: the rows and enclosure edges still read
too survey-like. Focused BJ audit crops make that failure more concrete, so
Cycle BL is the preferred queued follow-up over the older BK prompt. BL should
soften garden rows and lower the facade feel without adding walls or changing
topology. It was not rendered because the imagegen usage limit was reached. See
`../beechwood-qm-notebook-refine-cycle-bj-bk.md`.

Cycle BM makes the isomorphic transform explicit. The useful baseline is a 55%
playable-core crop plus a deterministic `y_squash ~= 0.40` lower-pitch cue
before final imagegen. That improves camera/zoom substantially on Kilteevan and
generalizes to Beechwood and Grove, but the remaining weakness is feature
semantics: paths, walls, garden rows, and soft boundaries still need better
upstream material controls. See
`../isomorphic-transform-calibration-cycle-bm.md`.

Cycle BN follows the user's lower-camera note by extending the Kilteevan source
far to the north before asking for a lower camera. E1 at 20 degrees is useful
but still high. E2 at 10-12 degrees is the first convincing roughly 50%-lower
camera result, with much larger facades/doors and source-backed far-north
background. The tradeoff is that lower tilt hardens garden edges and
wall/fence/boundary marks, so BN E2 should be treated as camera target proof
rather than recipe proof. See `../incremental-low-camera-cycle-bn.md`.

Cycle BO tests a final rectification step from BN E2. The useful decomposition
is to let the model create a low-camera, source-backed draft first, then ask
for a conservative orthographic/barrel-correction pass. BO E1 proves the
geometry works but overbuilds fences and walls. BO E2 is the preferred result:
less fisheye than BN E2, less fence/wall hardness than BO E1, and the low
camera mostly survives. See `../orthographic-rectification-cycle-bo.md`.

Cycle BP tests whether the BO style breakdown is an ordering problem. It is:
using BO E2 as the perspective/content base, a hard isomorphic grid as a
projection law, and the notebook look as the final pass gives better style
without losing the rectified low-oblique geometry. BP E2 is the preferred
visual candidate; BP E1 remains the stricter geometry reference. The remaining
failure is still garden/wall/planting semantics, not the art-last order. See
`../art-last-grid-check-cycle-bp.md`.

Cycle BQ tightens the definition of grid correctness. BP checked parallel
linework, but the user caught a more important gameplay failure: distant/top
trees were smaller than near trees, so sprites would need unknown y-dependent
scaling. BQ adds equal-size tree/sprite marker overlays as a constant-scale
audit. BQ E1 partially fixes the far-tree miniaturization, but hardens
vegetation and garden boundaries, so treat the audit as the durable result and
the render as a partial pass. See `../scale-lock-orthographic-cycle-bq.md`.

Cycle BR follows the relaxed-scale concept-art direction. It chooses Beechwood,
shrinks the playable area hard, raises the camera slightly, and uses door-fixed
style crops so close facades get fitted plank doors instead of black voids. BR
E1 is the current pass for this alternative branch: it better matches the
original notebook warmth/detail than the strict-grid repairs, but should not be
treated as runtime-safe strict isomorphic evidence. See
`../close-concept-relaxed-scale-cycle-br.md`.

Cycle BS uses the original notebook's door height as the camera/scale gate for
the relaxed concept-art branch. BS E2 is the better zoom target: it pulls back
20% from BS E1 while keeping readable fitted plank doors. See
`../door-height-calibration-cycle-bs.md`.

Cycle BT tests how to make BS E2 feel more like a used rural place rather than
a clean estate plan. The best direction is not surface dirt alone: sparse
practical clutter from BT E2 plus capped handmade irregularity from BT E3 is
the useful prompt recipe. See `../concept-realism-weathering-cycle-bt.md`.

Cycle BU runs that hybrid and one final tightening pass. BU E2 is the accepted
relaxed concept-art visual target for this branch: warm, worn, handmade, sparse
but lived-in, with fitted doors and the BS E2 scale preserved. It is edit-target
evidence rather than fresh one-shot recipe proof, so future pipeline work should
copy its prompt lessons without depending on prior rendered plates. See
`../concept-realism-convergence-cycle-bu.md`.

Cycle BV turns the BU result into a reusable Grove validation pipeline. BV E1
uses source map + local topology control + oblique cue + BU E2 style target
without a previous Grove render as edit target, and passes the separate-building
topology/door check. BV E2 is the one bounded correction and the preferred
visual output. See `../reproducible-pipeline-grove-cycle-bv.md`.

Cycle BZ reruns Grove through the stricter subagent-gated pipeline: clean
map-reader, deterministic control-builder, prompt-builder, render subagent with
one imagegen call, and independent audit. The audit verdict is PASS WITH
CAVEATS. The run proves the chain can preserve Grove's major geometry, doors,
camera, and BU concept-realism style, but it still over-materializes ambiguous
enclosure/field edges as continuous blocky stone walls. Treat BZ as proof that
the pipeline executes and preserves topology for Grove, not production batch
readiness. See `../reproducible-pipeline-grove-cycle-bz.md`.
