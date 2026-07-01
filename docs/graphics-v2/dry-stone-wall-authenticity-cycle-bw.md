# Dry-Stone Wall Authenticity Cycle BW

## Purpose

Cycle BW applies the Irish dry-stone wall reference rules to the current Grove
BV output. The goal is narrow: keep the Grove topology, doors, BU-style finish,
and camera, while replacing tidy block/cobblestone wall language with
authentic historic Irish fieldstone.

This is a bounded material pass, not a new map-to-image recipe test.

## Outputs

| ID | Image | Prompt | Report | Result |
| --- | --- | --- | --- | --- |
| E1 | `pipeline-experiments/idea-bw-e1-grove-dry-stone-wall-authenticity.png` | `pipeline-experiments/idea-bw-e1-grove-dry-stone-wall-authenticity.prompt.md` | `pipeline-experiments/idea-bw-e1-grove-dry-stone-wall-authenticity.report.md` | Topology safe, wall material still bead-chain |
| E2 | `pipeline-experiments/idea-bw-e2-grove-dry-stone-bead-chain-breakup.png` | `pipeline-experiments/idea-bw-e2-grove-dry-stone-bead-chain-breakup.prompt.md` | `pipeline-experiments/idea-bw-e2-grove-dry-stone-bead-chain-breakup.report.md` | Conservative intermediate; too subtle at normal zoom |
| E3 | `pipeline-experiments/idea-bw-e3-grove-hedgebank-dry-stone-breakup.png` | `pipeline-experiments/idea-bw-e3-grove-hedgebank-dry-stone-breakup.prompt.md` | `pipeline-experiments/idea-bw-e3-grove-hedgebank-dry-stone-breakup.report.md` | First visibly different boundary pass; darker/busier caveat |
| E4 | `pipeline-experiments/idea-bw-e4-grove-web-reference-dry-stone.png` | `pipeline-experiments/idea-bw-e4-grove-web-reference-dry-stone.prompt.md` | `pipeline-experiments/idea-bw-e4-grove-web-reference-dry-stone.report.md` | Real wall reference pass; preserves style but still over-walls Roscommon boundaries |

Comparison plate:

- `cartographic-comparisons/bw-grove-dry-stone-wall-comparison.png`

Source-backed references:

- `irish-dry-stone-wall-reference.md`
- `web-references/irish-dry-stone-walls/irish-dry-stone-wall-reference-sheet.png`

## Verdict

E3 is the strongest saved Grove image for testing visibly different boundary
material.

E1 and E2 were too subtle. They preserved topology and doors, but the walls
still read as regular stone chains at normal zoom. E3 changes the boundary
network more materially: minor garden edges become more overgrown, bank-like,
or partially buried, and the garden no longer reads as a fully continuous stone
grid.

E4 adds an explicit real-world dry-stone wall reference. It preserves the clean
BV composition better than E3, but still keeps too many ordinary boundaries as
continuous stone walls. That points to a larger prompt correction: for rural
Roscommon, use hedgerows, hedgebanks, banks, ditches, remnant hedges, and
stone-earthen banks as the normal boundary palette, with full dry-stone walls
reserved for supported local wall sections.

The remaining issue is not solved. Several close road-edge walls still show a
repeated large-stone rhythm that can read as a chunky bead chain, and E3 is
darker/busier than the BU/BV target. Future prompts should combine the E4
real-wall reference with the regional boundary prior, rather than simply asking
for every boundary wall to be made more authentic.

## Recommendation

Use BW E3 when judging whether the boundary treatment is visibly different, and
E4 when judging whether a real wall reference helps material fidelity. Keep BV
E2 as the pipeline transfer baseline. For future locations, include the
regional boundary prior and real dry-stone reference before the final material
pass rather than repairing walls after the image is otherwise finished.
