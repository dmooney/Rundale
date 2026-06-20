# Visual Client M1

The player-facing goal is a second Parish client that treats the game as a
graphics-first diorama instead of a text HUD with a preview panel. The first
milestone is deliberately modest: create a separate browser app that talks to
the existing Parish HTTP backend, asks for `/api/scene-state`, and renders that
contract onto a canvas with placeholders for the scene plate, hotspots, NPCs,
and missing/disabled states. The current Tauri/Svelte app remains the text,
debug, and control surface.

## Affected Subsystems

- `parish/apps/visual`: new graphics-first browser client. This milestone uses
  browser-native JavaScript, Node scripts, and Canvas 2D to avoid committing to
  a final rendering engine before the data contract is proven.
- `parish/crates/parish-server`: existing `/api/scene-state` and
  `/api/scene-asset/*` routes are consumed as-is. No backend changes are
  required for this milestone.
- `parish/apps/ui`: intentionally left alone except for the existing diorama
  proof work already in this branch.
- `docs/agent` / root docs: command documentation may need follow-up once the
  visual client graduates beyond a scaffold.

## Data Model

No new engine data model is required. The new app consumes the existing
`SceneState` JSON shape:

- `location_id`, `location_name`, `slug`, `variant`, `plate_url`, `indoor`, and
  `weather_overlay`
- `hotspots[]` with bounds and action metadata
- `slots[]` and assigned `npcs[]`
- `overflow_npcs[]`

The app owns a small client-side view model for loading, error, disabled, and
scene-present states. It also posts movement text to `POST /api/command`, then
refreshes `/api/scene-state` for the same browser session. Later milestones can
replace the Canvas 2D renderer with PixiJS, Phaser, Three.js, or another engine
without changing the server contract.

## Observable Signal

The harness-visible backend signal is `/scene` with the `diorama` flag enabled:
the output must include the current scene, plate, hotspots, and NPC slots. The
browser-visible signal is the standalone app rendering that same information on
a canvas and surrounding inspector surface after its own command bridge moves
the browser session to an authored scene.

## Feature Flag

The backend data remains gated by `config.flags.is_enabled("diorama")`. The
visual app does not introduce a new runtime flag; it reports the disabled/null
state when the backend returns `null`.
