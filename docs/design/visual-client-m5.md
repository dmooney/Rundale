# Visual Client M5

This milestone makes the separate graphics-first Parish client playable from
the sidebar as well as from the Canvas. The Hotspots and People panels become
quick-action controls backed by the same scene display model that drives Canvas
hit testing.

## Affected Subsystems

- `parish/apps/visual`: renders hotspot and NPC lists as buttons, wires those
  buttons to the existing Canvas activation functions, and keeps action labels
  covered by pure unit tests.
- `parish/crates/parish-server`: consumed as-is through `/api/scene-state`,
  `/api/scene-asset/*`, and `/api/command`.
- `parish/apps/ui`: intentionally untouched for this milestone.

## Interaction Model

Hotspot quick actions reuse `hotspotCommand`:

- inspect hotspots write their authored text to the command log and transcript;
- travel hotspots fill the command input, submit the movement command, and
  refresh scene state.

NPC quick actions reuse `npcCommand`: they select/highlight the NPC, prepare the
`talk to ...` command, and append a transcript selection without submitting
dialogue automatically.

## Data Model

No backend data model changes are required. The visual client adds a tiny
`action-list` helper module for stable hotspot/person button text so labels can
be tested without a browser DOM.

## Observable Signal

The harness signal remains backend scene-state availability for Crossroads and
Darcy's Pub. The browser signal is that sidebar buttons can inspect the
Crossroads wall, travel to Darcy's Pub, and prepare a talk command for a present
NPC.

## Feature Flag

No new flag. Backend scene-state remains gated by the existing `diorama` flag.
