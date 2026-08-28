# Grove Cycle BZ Subagent-Gated Pipeline Proof

Cycle BZ is the first fully subagent-gated proof run of the BU-style
Kilteevan exterior pipeline on Grove. It uses fresh clean-context stages for
map reading, deterministic control generation, prompt handoff, one render
subagent imagegen call, and independent audit.

## Source And Goal

- Source crop: `grove-map-target-site-crop.png`
- Visual style target: `authorities/beechwood-concept-realism-bu-e2.png`
- Primary control: `pipeline-experiments/idea-bz-grove-subagent-control-literal-paint-control.png`
- Camera cue: `pipeline-experiments/idea-bz-grove-subagent-control-oblique-raw-warp.png`
- Render: `pipeline-experiments/idea-bz-grove-subagent-bu-style.png`
- Comparison plate: `cartographic-comparisons/bz-grove-subagent-pipeline-proof-comparison.png`

The goal was not another polish loop. The goal was to prove whether the written
pipeline can be run from source map to candidate plate with independent staged
artifacts and an audit verdict.

## Subagent Chain

1. Map-reader subagent wrote
   `pipeline-experiments/idea-bz-grove-subagent-map-reader-notes.md`.
2. Control-builder subagent ran `prototype_map_controls.py` and wrote
   `pipeline-experiments/idea-bz-grove-subagent-control.report.md`.
3. Prompt-builder subagent wrote
   `pipeline-experiments/idea-bz-grove-subagent-bu-style.prompt.md`.
4. Render subagent called imagegen exactly once and copied the result to
   `pipeline-experiments/idea-bz-grove-subagent-bu-style.png`; see
   `pipeline-experiments/idea-bz-grove-subagent-bu-style.render-report.md`.
5. Audit subagent wrote
   `pipeline-experiments/idea-bz-grove-subagent-bu-style.audit-report.md`.

## Audit Verdict

Verdict: **PASS WITH CAVEATS**.

The audit says this is enough to count as a real subagent-gated pipeline proof
run for Grove. The render preserves the major Grove topology: the pale lane
enters from the north/northeast, runs down the east side, and bends into the
yard; the northwest planted enclosure remains planted and subdivided; B1, B2,
B3, and tiny B4 remain distinct; source-negative features such as church, shop,
water, bridge, people, livestock, smoke, UI, and labels are absent. Doors are
visible on the accessible buildings, and the style/perspective are close to the
BU concept-realism target.

The caveat is material semantics. Ambiguous Grove enclosure and field edges are
over-promoted into continuous, regular stone walls, especially around the
orchard and lower/right field edges. This is attractive but stricter than the
map supports and still too block-like/continuous for the Roscommon boundary
rules.

## Disposition

Cycle BZ proves the pipeline can execute end to end with clean subagents and can
preserve Grove's major geometry, doors, camera, and style in one render call.
It does **not** prove production batch readiness. The next proof run must add a
stricter boundary-material gate: ambiguous boundaries should default to hedges,
banks, ditches, intermittent trees, wood fencing, or very short broken
dry-stone remnants, not continuous stone wall grids.
