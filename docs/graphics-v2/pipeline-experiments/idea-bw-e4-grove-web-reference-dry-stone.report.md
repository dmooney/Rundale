# Cycle BW E4 Grove Web-Reference Dry-Stone Report

## Inputs

- Edit target: `idea-bv-e2-grove-bv-e1-bu-style-tighten.png`
- Material reference:
  `../web-references/irish-dry-stone-walls/irish-dry-stone-wall-reference-sheet.png`
- Prompt: `idea-bw-e4-grove-web-reference-dry-stone.prompt.md`
- Generated cache source:
  `/Users/dmooney/.codex/generated_images/019f0fee-e45e-7890-85a7-ed0dc4099c99/ig_0cc2653742ce27af016a4435d3c0a08195a10bf9d0343a8f41.png`

## Result

E4 is a partial material improvement and a useful negative result.

Providing the real-world wall reference did help constrain the edit back toward
the cleaner BV composition instead of the darker E3 overgrowth. Doors, Grove's
building group, road curve, gates, and overall BU-style illustration remain
intact.

The wall-material problem is not solved. The model still treats many boundary
runs as continuous stone walls and still collapses close walls into chunky
rounded stones rather than slabby gapped fieldstone. This suggests the prompt
problem is not only the phrase "dry-stone wall"; it is also the assumption that
every boundary should remain a wall.

## Audit

- Grove topology: pass.
- Doors/thresholds: pass.
- Style preservation: pass.
- Use of real wall reference: partial; some roughness improves, but slabby
  interlocked construction is still weak.
- Regional boundary authenticity: fail; too many ordinary Roscommon field and
  garden lines remain continuous stone walls.

## Lesson

For County Roscommon prompts, apply a regional boundary prior before asking for
wall material:

- hedgerows, hedgebanks, banks, ditches, and remnant hedges as the default,
- stone-earthen banks where a hedge/bank has exposed stone facing,
- short full dry-stone walls near gates, yards, buildings, and well-drained or
  rocky patches,
- continuous full stone walls only when source/control or local landscape
  context supports them.

Future repairs should not merely "make walls more authentic." They should
replace many ordinary boundary lines with hedges/banks/ditches and reserve
the real-wall reference for the remaining stone sections.
