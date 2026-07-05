# Graphics V2 Handover: Notebook Q/M Plate Work

Date: 2026-06-28

## Current Goal

Replicate the original illustrated parish notebook background-plate look while
preserving the Cycle M/Q map-derived topology accuracy. The target is still a
reproducible map-to-plate pipeline, not a hand-authored Grove or Beechwood
prompt.

## Current Best State

The strongest Beechwood visual target is:

- `pipeline-experiments/idea-bj-beechwood-q-notebook-repaint.png`

BJ is a bounded repaint from Beechwood Q/M. It improves the notebook feel
without breaking the connected compound, diagonal road, attached garden, lower
building group, major tree masses, or open-field zones. It also improves doors,
rough ink, watercolor field texture, and no-chimney discipline.

Do not call BJ one-shot recipe proof. It starts from prior rendered plates and
therefore is visual-target evidence only.

## Remaining Gap

The main BJ failure is local and concrete: the garden still looks too regular,
rectangular, and survey-like. It should feel like planted ground seen in a
loose ink-and-watercolor notebook plate, not like a crisp plan drawing. Rows may
remain as soft planting texture, but they should not become little walls,
masonry caps, extra paths, or hard plot geometry.

The focused audit is:

- `beechwood-bj-visual-audit-crops.md`

Use those crops before writing any new prompt around BJ.

## Preferred Next Run

Run Cycle BL next, not BK. BK is saved, but BL supersedes it because it includes
focused garden/facade audit crops and states the remaining failure more clearly.

Queued prompt:

- `pipeline-experiments/idea-bl-beechwood-bj-crop-aware-soft-garden.prompt.md`

Queued report:

- `pipeline-experiments/idea-bl-beechwood-bj-crop-aware-soft-garden.report.md`

Expected output path:

- `pipeline-experiments/idea-bl-beechwood-bj-crop-aware-soft-garden.png`

Run BL in a clean-context subagent. The subagent should receive only the saved
prompt, the listed reference images, and the instruction to save the output and
write a short audit report. Print the full prompt to chat after the cycle.

## Non-Negotiables For The Next Cycle

- Keep the map/topology target north-up.
- Preserve the BJ/Q/M layout; do not add roads, paths, bridges, rivers,
  churches, carts, people, animals, labels, or UI.
- Do not add smoke, fog, weather effects, or other animated-layer content.
- Do not use Grove-specific or Beechwood-specific prose as a reusable template.
  Location-specific evidence is acceptable only when produced by the same
  reproducible map-reader/control process for every crop.
- Keep the "doors on openings" rule literal: every visible person-sized dark
  vertical opening must contain a visible wooden plank door fitted into that
  opening, with a threshold connected to yard or road.
- Keep the no-chimney rule strict. If a roof mark might read as a chimney,
  vent, stack, random nub, or stone block, it is a failure unless the source
  evidence specifically requires it.
- Treat administrative/dotted survey boundaries as non-physical unless
  corroborated by other map evidence. Do not let them become hedges, walls, or
  roads.

## Audit Checklist

After BL or any successor render, inspect both whole plate and fixed crops:

- topology: connected compound, diagonal road, attached garden, lower building
  group, open fields, and tree masses still match BJ/Q/M;
- garden: softer planting texture, fewer hard rectangles, no added walling;
- camera: lower 3/4 isomorphic feel, not a top-down survey board;
- style: sepia ink, hand-drawn line wobble, watercolor wash, paper texture,
  rough roads, and varied grass comparable to the original notebook sample;
- buildings: every visible walkable facade has a readable door/opening with a
  door in it;
- roofs: no chimneys, vents, random roof protrusions, smoke, or roof-wall
  confusion;
- semantics: no church, graveyard, bridge, river, sign text, UI, people,
  animals, carts, barrels, or decorative props copied from style references.

## Files To Read First

1. `docs/graphics-v2/AGENTS.md`
2. `docs/graphics-v2/beechwood-qm-notebook-refine-cycle-bj-bk.md`
3. `docs/graphics-v2/beechwood-bj-visual-audit-crops.md`
4. `docs/graphics-v2/pipeline-experiments/idea-bl-beechwood-bj-crop-aware-soft-garden.prompt.md`
5. `docs/graphics-v2/pipeline-experiments/idea-bl-beechwood-bj-crop-aware-soft-garden.report.md`

## Commit Scope Note

This folder intentionally contains many generated research artifacts. Keep
future commits scoped to `docs/graphics-v2` and any relevant repo-level
learnings. Do not stage unrelated runtime or UI files while working on graphics
experiments.
