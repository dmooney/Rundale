# Reproducible Grove Pipeline Cycle BV

## Purpose

Cycle BV tests whether the BU concept-realism result can become a reproducible
map-to-background pipeline for another location. Grove is the validation target
because its topology is different from Beechwood: separate buildings around a
working yard, with a garden/orchard block and road exits, not a connected
courtyard compound.

The test intentionally avoids using an existing Grove render as the first edit
target. BV E1 is generated from reusable inputs:

- Grove source map,
- Grove core topology control,
- Grove oblique camera cue,
- BU E2 as style/material target,
- door-fixed single-building references.

BV E2 is the one bounded correction allowed by the pipeline.

## Outputs

| ID | Image | Prompt | Report | Result |
| --- | --- | --- | --- | --- |
| E1 | `pipeline-experiments/idea-bv-e1-grove-reproducible-bu-style.png` | `pipeline-experiments/idea-bv-e1-grove-reproducible-bu-style.prompt.md` | `pipeline-experiments/idea-bv-e1-grove-reproducible-bu-style.report.md` | Direct pipeline pass; topology/doors pass, style slightly clean/high |
| E2 | `pipeline-experiments/idea-bv-e2-grove-bv-e1-bu-style-tighten.png` | `pipeline-experiments/idea-bv-e2-grove-bv-e1-bu-style-tighten.prompt.md` | `pipeline-experiments/idea-bv-e2-grove-bv-e1-bu-style-tighten.report.md` | Preferred Grove result; one bounded style/scale correction |

Comparison plate:

- `cartographic-comparisons/bv-grove-reproducible-pipeline-comparison.png`

Pipeline note:

- `map-to-bu-style-reproducible-pipeline.md`

## Verdict

The pipeline works on Grove with one bounded correction.

BV E1 is the important recipe proof: without a previous Grove render as edit
target, it keeps Grove's separate buildings, working yard, road exits, garden
block, gates, walls, and readable fitted doors. It applies the BU style family
well enough to show transfer, but the result is a little cleaner and higher
than BU E2.

BV E2 is the preferred visual output: it preserves the Grove topology and doors
while adding the warmer, rougher, more worn BU finish. It still reads as Grove,
not Beechwood. The garden remains auditable, roads remain open, and the
building group does not collapse into a connected courtyard compound.

## Caveats

- BV E2 has a tiny roof-nub/vent-like mark on the taller east/right building.
  It is small enough to document rather than spend another imagegen pass.
- The garden is still more organized than the original concept-art sample, but
  loosening it further risks losing source readability.
- The topology control itself is not a raw deterministic vector map; it is a
  reusable generated control from the earlier AE crop-scale branch. For larger
  batch use, the control-generation stage should be made more explicit and
  repeatable.

## Recommendation

Use BV as the current reproducible pipeline baseline:

```text
source map + reusable local topology control + oblique camera cue
  -> direct BU-style render
  -> at most one bounded BU-style/door/scale correction
```

The next validation should run the same BV prompt shape on one more unrelated
location before treating it as batch-ready.
