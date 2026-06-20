# Visual Client M4

This milestone gives the separate graphics client a compact play transcript.
The player can see a short history of their commands, world responses, inspect
results, and sprite selections while continuing to interact with the Canvas
scene. This keeps the app useful as an alternate graphics-first client rather
than a one-line debug inspector.

## Affected Subsystems

- `parish/apps/visual`: adds a transcript model, renders it in the browser UI,
  and records command, world, inspect, and sprite-selection events.
- `parish/crates/parish-server`: consumed as-is through `/api/scene-state`,
  `/api/scene-asset/*`, and `/api/command`.
- `parish/apps/ui`: intentionally untouched for this milestone.

## Data Model

No backend data model changes are required. The visual client keeps a local
bounded array of transcript entries:

- `kind`: `player`, `world`, `inspect`, `selection`, or `system`;
- `label`: short display label;
- `text`: the visible entry body.

Entries are capped client-side to keep repeated play from growing the DOM
without bound.

## Observable Signal

The harness signal remains backend scene-state availability for Crossroads and
Darcy's Pub. The browser signal is the visual client retaining multiple recent
entries after movement, inspect, and sprite-selection interactions.

## Feature Flag

No new flag. Backend scene-state remains gated by the existing `diorama` flag.
