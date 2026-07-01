# Cycle BW E2 Grove Dry-Stone Bead-Chain Breakup Report

## Inputs

- Edit target: `idea-bw-e1-grove-dry-stone-wall-authenticity.png`
- Prompt: `idea-bw-e2-grove-dry-stone-bead-chain-breakup.prompt.md`
- Generated cache source:
  `/Users/dmooney/.codex/generated_images/019f0fee-e45e-7890-85a7-ed0dc4099c99/ig_02c64d1b8064abbb016a4412ad795081958e14dc29fe973e01.png`

## Result

E2 is a cleaner Grove image than E1, but it is too conservative as a
dry-stone-wall correction.

It preserves the Grove layout, road exits, open yard, garden block, gates,
separate building group, and fitted plank doors. The walls move in the right
direction: more gaps, more mixed stone sizes, more moss/weeds, and less clean
rectangular blockwork than the BV baseline.

It is not a perfect wall-material target. Some foreground and central wall runs
still have a chunky regularity and occasional large-stone chain effect. The
image is still usable as the current preferred Grove plate because the building
topology and door discipline remain intact, and the wall failure is narrower
than in E1.

## Audit

- Grove topology: pass.
- Doors/thresholds: pass.
- No added buildings/roads/props/UI/water/church/graveyard: pass.
- Wall authenticity: pass with caveat. Stronger than BV/E1, but the closest
  walls still need less repeated top-stone rhythm in a future recipe.
- Recommended status: useful conservative intermediate. It was superseded by
  E3 for testing a visibly different boundary treatment.

## Prompt Lesson

The useful wording is the targeted bead-chain breakup:

- forbid identical gray beads and bead-like top rows,
- request mixed angular stones, wedge stones, slab fragments, and larger
  bouldery pieces,
- allow minor internal boundaries to become low broken banks, hedges, ditches,
  or overgrown wall remnants rather than full continuous walls.

Keep that language in the reusable Grove/BU pipeline, but continue auditing the
nearest wall edges at crop scale.
