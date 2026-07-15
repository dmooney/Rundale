# OS Map Legend Refinement Result CA4P

Goal: refine the OS 6-inch map reference pack until a clean-context map reader
can annotate the Grove crop without location-specific prompting.

## Accepted Reference Pack

Use these inputs together:

- `os-6inch-map-key-reference-sheet.png` — clean OS key, unannotated.
- `grove-map-hint-examples-ca4m.png` — broad general examples and YES/NO
  contrast cards.
- `grove-hard-symbol-micro-reference-ca4p.png` — enlarged micro-reference for
  the two unstable classes.
- `docs/graphics-v2/grove-map-target-site-crop.png` — annotation target.

## Best Test Output

- `docs/graphics-v2/pipeline-experiments/idea-ca4p-grove-ca4p-micro-reference-annotation.png`
- `docs/graphics-v2/pipeline-experiments/idea-ca4p-grove-ca4p-micro-reference-annotation.report.md`

## Result

CA4P is the first clean-context pass that handled the hard classes acceptably:

- the faint western paired route was marked as an **unfenced path / track
  candidate** rather than collapsed into the bold dotted administrative line,
- bold dotted linework stayed in a separate **dotted administrative / survey
  boundary** class,
- the southern boundary feature with mixed tree and irregular vegetation marks
  was marked as a **vegetated rough strip** candidate,
- the regular planted enclosure remained a **planted / mixed enclosure** rather
  than being over-promoted to rough vegetation.

The hard classes remain low-confidence candidates, which is appropriate for
the Grove crop resolution and blur. Treat CA4P as the reference-pack pattern to
reuse before attempting downstream topology control or final art generation.

## Failed Approaches To Avoid

- Annotating or marking up the OS key itself.
- Using a single combined reference image where the hard examples are too
  small to read.
- Giving only a broad source-crop example for the path: it causes the model to
  confuse the faint paired route with stronger dotted administrative linework.
- Listing every possible symbol class as something that must appear; this
  makes the model force rough vegetation onto ordinary yard clutter or planted
  enclosure texture.
