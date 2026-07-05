# BZ Grove Subagent BU-Style Proof Audit

## Verdict

PASS WITH CAVEATS.

The Grove render is enough to count as a real subagent-gated pipeline proof run for this location: the saved artifacts show a clean map-reader note pass, a deterministic control-builder pass, a prompt handoff, and a render subagent report stating imagegen was called exactly once. Visually, the candidate preserves the main Grove homestead topology and avoids the major source-negative inventions. It is not an unconditional pass because boundary material semantics are over-asserted: ambiguous enclosure and field lines become continuous blocky stone walls more often than the source supports.

## Geometry Checks

- Eastern/northeastern lane: pass. A pale lane enters from the north/northeast, runs down the east side of the homestead, and remains the dominant route.
- Bend into yard: pass. The lane bends toward the farmyard and connects plausibly to the compound.
- Northwest planted enclosure/orchard: pass with caveat. The enclosure is northwest of the yard, planted, subdivided, and tree-filled, but it reads as a hard-walled garden rather than an ambiguous orchard/enclosure with hedges, ditches, banks, or softer edges.
- B1 north/south range near lane: pass. The substantial north/south building sits near the lane on the east side of the group, separate from B2.
- B2 long east/west range south of enclosure: pass. The long range sits south of the planted enclosure and anchors the yard.
- B3 small southwest outbuilding: pass. A small southwest building is present and subordinate.
- Optional B4 tiny north enclosure structure: pass with caveat. A tiny structure appears inside/near the northern enclosure edge and stays secondary.
- Source-negative items avoided: mostly pass. No church, shop, water, bridge, people, livestock, carts, smoke, UI, or map labels are visible. The main caveat is over-materialization of source-ambiguous boundary lines into continuous stone walls.

## Door And Facade Checks

Pass. Every accessible building has a visible fitted wooden door or door-like plank opening on a readable facade: B2 has multiple front doors, B1 has a visible side/front door facing the lane/yard, B3 has a clear southwest door, and B4 has a tiny visible door. I do not see a pure black void standing in for a required doorway.

## Style And Perspective Checks

Pass. The render closely matches BU-like concept realism: hand-painted, textured vegetation, pale plaster/stone buildings, thatch/slate roof language, muddy lane, and readable game-background detail. The camera is low 3/4 near-orthographic and closer/playable, not a flat survey board. It is still slightly polished and composed, but not enough to fail the perspective target.

## Historical Semantics And Materials

Pass with caveats. The render avoids labels, dotted/admin line symbols, churches, shops, water, bridges, people, livestock, smoke, and UI. It uses plausible 1820s rural farm materials and keeps the scene vernacular.

The significant defect is boundary treatment. Many enclosure and field edges are rendered as continuous, regular gray stone walls with block-like coping. The source and notes only support walls/hedges/ditches/plot edges ambiguously, and recent graphics guidance says Roscommon boundaries should default more often to hedges, banks, ditches, intermittent trees, wood fencing, and short rough dry-stone sections. The candidate's walls are attractive but too uniform and too continuous for a strict source-fidelity proof.

## Reproducibility And Proof Checks

The proof chain is documented well enough for a pipeline run:

- Map-reader notes are saved in `idea-bz-grove-subagent-map-reader-notes.md`.
- Control-builder report is saved in `idea-bz-grove-subagent-control.report.md` and documents the deterministic `prototype_map_controls.py` command.
- Prompt handoff is saved in `idea-bz-grove-subagent-bu-style.prompt.md` with the expected manifest and constraints.
- Render report is saved in `idea-bz-grove-subagent-bu-style.render-report.md` and says the render subagent called imagegen exactly once, then copied the output to the candidate path.

That is enough to prove the subagent-gated pipeline executed for Grove. The visual result proves the pipeline can preserve the main Grove topology, but the boundary-material caveat means it should not be treated as a final batch recipe without a stricter boundary-material gate or repair.

## Top 3 Concrete Defects

1. Ambiguous enclosure/field boundaries are over-promoted into continuous stone walls, especially around the orchard and lower/right field edges.
2. Stone wall rendering is too uniform and block-like in places, closer to regular coping/ashlar than irregular dry-fit local stone, banks, hedges, or ditches.
3. The planted enclosure reads as a formal walled kitchen garden rather than a softer orchard/planted enclosure with mixed trees, hedges, scrub, and internal beds.
