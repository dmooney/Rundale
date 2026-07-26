# Parish Notebook UI

Status: Retired historical experiment. Both the prototype and its successor
Pixi implementation were removed; this document is retained as context only.

## Goal

Replace the current primary Rundale game HUD with a new from-scratch interface
modeled on the approved `Illustrated Parish Notebook` concept art. This is not
a parchment reskin of the existing chat/map/sidebar layout. The new root screen
should feel like a playable historical notebook laid over a visible parish
scene: paper tabs, ink labels, nearby people, selected-person notes, compact
action stamps, and a natural-language intent line.

The exterior-art pipeline is not a dependency. The first implementation may use
the existing live map or a neutral paper stage as the world surface, but the UI
composition itself should match the concept direction.

## Product Target

Reference:

- `docs/graphics-v2/illustrated-parish-notebook.png`
- `docs/graphics-v2/illustrated-parish-notebook-prompt.md`
- `docs/graphics-v2/concept-7a-conversation-lens.png`
- `docs/graphics-v2/concept-7c-roads-and-schedules.png`

The notebook target has these stable pieces:

- a wide world-first center stage;
- a thin top ribbon with location, time, weather, and notebook title;
- a left rail of nearby people;
- a right bound notebook page for the selected person or place notes;
- side tabs for Notes, People, Places, Rumours, and Journal;
- bottom action stamps for Talk, Ask, Help, Observe, and Leave / Map;
- a bottom intent strip that remains the primary input affordance.

## Affected Frontend Areas

- `parish/apps/ui/src/routes/+page.svelte`: replace the visible root game
  composition with the new notebook shell.
- `parish/apps/ui/src/components/notebook/`: new component family for the
  shell, world stage, top ribbon, nearby rail, notebook page, tabs, and action
  desk.
- `parish/apps/ui/src/components/InputField.svelte`: reuse the existing input
  behavior, or extract its logic if the visual shell needs a new wrapper.
- `parish/apps/ui/src/components/MapPanel.svelte` and
  `FullMapOverlay.svelte`: use as live map/world affordances where helpful, but
  do not make the old right-column minimap/sidebar layout visible.
- `parish/apps/ui/src/stores/game.ts`, `stores/travel.ts`, `stores/save.ts`,
  `stores/debug.ts`: read existing state; avoid new transport forks.
- `parish/apps/ui/src/app.css`: add notebook-level tokens and global paper
  texture only where needed.

## Data Model

No Rust type changes are required for the first pass.

The frontend can derive the first selected/focused notebook subject from
available state in this order:

1. explicit UI selection in the nearby rail;
2. most recently addressed NPC if the store exposes it later;
3. first NPC present at the current location;
4. no selected person, showing place notes instead.

Known facts are intentionally conservative in the first pass. Use existing
message/name/location hints and current NPC metadata only; do not invent a new
memory API during the UI rebuild.

## Interaction Model

- Selecting a person in the left rail opens that person on the right notebook
  page.
- Talk / Ask / Help / Observe are stamp-like controls that focus the intent
  line with a natural-language starter, rather than bypassing typed intent.
- Leave / Map opens existing map affordances.
- Typed input continues to submit through the existing IPC pipeline.
- Existing keyboard shortcuts remain where they do not conflict with typing.
- Mobile collapses the notebook into stage, notes, and desk regions without
  duplicating sidebars.

## Visual Rules

- Use paper, ink, tabs, stitched/spiral hints, rough borders, and restrained
  illustrated details.
- Avoid the dark RPG HUD look from the earlier 7A/7C concepts.
- Avoid marketing-page hero composition, giant portraits, modals covering the
  world, and old chat-app visual hierarchy.
- Keep text dense but legible; do not let handwritten styling undermine
  readability.
- The UI may use small sketches/icons, but buttons should remain predictable
  and accessible.

## Observable Proof

The proof should show:

- the new root screen, not the old chat/sidebar/map layout;
- live world status in the top ribbon;
- people present in the left rail;
- a selected person or place-note page on the right;
- action stamps and intent input at the bottom;
- a submitted command still producing game output;
- no visible overlap at desktop and mobile viewport sizes.
