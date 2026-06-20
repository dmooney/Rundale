# Visual Client M6

This milestone fixes the standalone visual client's desktop layout so the scene
stage behaves like a stable graphics viewport. The inspector can be taller than
the screen, but it should scroll on its own rather than stretching the whole CSS
grid and pushing the Canvas downward.

## Affected Subsystems

- `parish/apps/visual`: updates CSS layout rules for the graphics shell, stage,
  and inspector; keeps mobile as a normal stacked document.
- `parish/crates/parish-server`: consumed as-is through `/api/scene-state`,
  `/api/scene-asset/*`, and `/api/command`.
- `parish/apps/ui`: intentionally untouched for this milestone.

## Layout Model

Desktop uses a viewport-bound shell:

- the shell is exactly `100vh` tall and hides page-level overflow;
- the stage is `100vh` tall and centers the 16:9 Canvas inside it;
- the inspector is capped at `100vh` and owns vertical scrolling.

Mobile resets those constraints so the page remains a single-column scrolling
document, with the stage first and the inspector below it.

## Data Model

No backend data model changes are required.

## Observable Signal

The harness signal remains backend scene-state availability for Crossroads and
Darcy's Pub. The browser signal is that a desktop screenshot shows the Canvas in
the first viewport while the inspector has scrollable content, and a mobile
viewport remains stacked and scrollable.

## Feature Flag

No new flag. Backend scene-state remains gated by the existing `diorama` flag.
