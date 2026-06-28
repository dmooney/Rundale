# Beechwood Q/M Notebook Refinement Cycle BJ-BK

## Purpose

Cycle Q remains the best Beechwood candidate for the goal of matching the
original illustrated parish notebook look while preserving the Cycle M
map-derived topology. It keeps the connected compound, diagonal road, attached
garden, lower building group, tree masses, and open-field zones, but it still
reads too clean, high, and survey-like compared with the original notebook
sample.

Cycles BJ-BK test the bounded-repaint route recommended by BE-BH:

```text
Cycle M/Q topology target
  -> bounded notebook repaint preserving topology
  -> focused local/style repairs for concrete failures
```

These cycles are visual-target evidence, not one-shot recipe proof, because
they start from previous rendered plates.

## BJ: Bounded Repaint From Beechwood Q

BJ used:

- `pipeline-experiments/idea-q-beechwood-camera-refinement-notebook-style.png`
  as edit target and primary topology/style target,
- `pipeline-experiments/idea-m-beechwood-admin-topdown-cleaned.png` as topology
  veto,
- `illustrated-parish-notebook.png` as style-only reference,
- `pipeline-experiments/idea-bh-grove-bg-upper-structure-repair.png` as a
  no-UI notebook-style texture reference only,
- cleaned single-building slate and thatch crops plus field/wall and roof/wall
  material crops.

Output:

- `pipeline-experiments/idea-bj-beechwood-q-notebook-repaint.png`
- `pipeline-experiments/idea-bj-beechwood-q-notebook-repaint.prompt.md`
- `pipeline-experiments/idea-bj-beechwood-q-notebook-repaint.report.md`

Result: useful improvement. BJ preserves the Beechwood Q/M layout at whole-plate
scale: the diagonal road, connected L/U-shaped compound, courtyard, attached
rectangular garden, lower/foreground building group, left/top-left tree mass,
and open-field zones remain recognizable. It also improves the notebook feel
with heavier sepia ink, mottled watercolor fields, more legible doors and
facades, scumbled roads, darker tree masses, and no obvious chimneys, smoke,
people, animals, UI, labels, bridge, river, church, carts, barrels, or loose
props.

Caveat: the garden still reads too tidy and plan-like. Some enclosure and
garden edges remain more regular and wall-like than the original notebook
sample. BJ is closer to the target, but not the finish line.

## BK: Queued Soft-Garden / Lower-Facade Refinement

BK was designed as a conservative edit from BJ, with a narrower target:

- keep the Beechwood BJ/Q/M topology,
- lower the facade feel a little,
- roughen roads, fields, walls, and vegetation,
- soften the garden rows and beds without adding walling,
- preserve doors-on-openings and no-chimney discipline.

The exact prompt has been saved as:

- `pipeline-experiments/idea-bk-beechwood-bj-lower-notebook-soft-garden.prompt.md`

The render did not complete because the imagegen usage limit was reached. The
queued report is:

- `pipeline-experiments/idea-bk-beechwood-bj-lower-notebook-soft-garden.report.md`

When credits are available, rerun BK in a clean-context subagent and save the
output as:

- `pipeline-experiments/idea-bk-beechwood-bj-lower-notebook-soft-garden.png`

## Focused BJ Crop Audit

After BK was queued, a local crop audit made the remaining failure more
concrete. The audit crops are listed in:

- `beechwood-bj-visual-audit-crops.md`

The important finding is that BJ's main unsolved gap is local to the garden and
enclosure rendering. The connected compound, lower buildings, doors, roof
discipline, and broad topology are doing useful work; the garden still looks
too regular, rectangular, and survey-like.

## BL: Crop-Aware Soft-Garden Prompt

BL is the preferred next prompt when imagegen credits return. It supersedes BK
as the next run because it includes focused garden/facade audit crops and names
the failure more precisely.

The exact queued prompt is:

- `pipeline-experiments/idea-bl-beechwood-bj-crop-aware-soft-garden.prompt.md`

The queued report is:

- `pipeline-experiments/idea-bl-beechwood-bj-crop-aware-soft-garden.report.md`

When credits are available, run BL in a clean-context subagent and save the
output as:

- `pipeline-experiments/idea-bl-beechwood-bj-crop-aware-soft-garden.png`

## Current Recommendation

Treat BJ as the current Beechwood Q/M visual target, with the explicit caveat
that it still needs softer, less survey-like garden rendering. Do not promote
BJ as a one-shot recipe.

The next imagegen pass should be BL: bounded edit, crop-aware soft-garden
repair, no new layout, no added walls, no semantic copying from the notebook
sample, and topology preservation winning over style pressure.
