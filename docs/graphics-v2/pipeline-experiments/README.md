# Pipeline Experiments

Clean-context image-generation experiments for the historic-map-to-background
plate pipeline. Each experiment has:

- a generated PNG,
- the exact prompt used,
- a short report.

No experiment used hand-authored per-location road, wall, river, building, or
landmark hints.

## Outputs

| ID | Image | Prompt | Report | Result |
| --- | --- | --- | --- | --- |
| A | `idea-a-map-only.png` | `idea-a-map-only.prompt.md` | `idea-a-map-only.report.md` | Best source fidelity; pass with caveat |
| B | `idea-b-oblique-warp.png` | `idea-b-oblique-warp.prompt.md` | `idea-b-oblique-warp.report.md` | Fail |
| C | `idea-c-semantic-mask.png` | `idea-c-semantic-mask.prompt.md` | `idea-c-semantic-mask.report.md` | Pass |
| D | `idea-d-extruded-blockout.png` | `idea-d-extruded-blockout.prompt.md` | `idea-d-extruded-blockout.report.md` | Pass with caveat |
| E | `idea-e-oblique-ink-warp.png` | `idea-e-oblique-ink-warp.prompt.md` | `idea-e-oblique-ink-warp.report.md` | Fail |
| F | `idea-f-linework-control.png` | `idea-f-linework-control.prompt.md` | `idea-f-linework-control.report.md` | Control-path pass, lower source fidelity |
| G | `idea-g-raw-map-control-02.png` | `idea-g-raw-map-control-02.prompt.md` | `idea-g-raw-map-control-02.report.md` | Fail |
| H | `idea-h-grove-raw-style-crops.png`, `idea-h-beechwood-raw-style-crops.png` | `idea-h-*.prompt.md` | `idea-h-*.report.md` | Safer than full style scene, but still prop/style leakage risk |
| I | `idea-i-grove-strict-map-fidelity.png`, `idea-i-beechwood-strict-map-fidelity.png` | `idea-i-*.prompt.md` | `idea-i-*.report.md` | Cleaner semantics, but over-compressed footprints |
| J | `idea-j-grove-clean-style-swatches.png`, `idea-j-beechwood-clean-style-swatches.png` | `idea-j-*.prompt.md` | `idea-j-*.report.md` | Current paired pass |
| K | `idea-k-grove-map-reader-guided.png`, `idea-k-beechwood-map-reader-guided.png` | `idea-k-*.prompt.md` | `idea-k-*.report.md` | Current best for building interpretation |
| L | `idea-l-*-topdown-cleaned.png`, `idea-l-*-two-step-isomorphic.png` | `idea-l-*.prompt.md` | `idea-l-*.report.md` | Promising; stronger central topology, needs native 16:9 discipline |
| M | `idea-m-*-admin-topdown-cleaned.png`, `idea-m-*-admin-two-step-isomorphic.png` | `idea-m-*.prompt.md` | `idea-m-*.report.md` | Current best two-step path; suppresses non-physical admin boundaries |

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
