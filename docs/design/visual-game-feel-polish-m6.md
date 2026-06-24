# visual-game-feel-polish-m6

The player experience for this pass is a quieter, more game-like visual client:
the world remains full-screen, non-game controls stop reading as dashboard
chrome, the log becomes secondary, and hover/click feedback happens in the
scene rather than in surrounding panels.

## Affected Subsystems

- `parish/apps/visual`: HTML/CSS, Pixi renderer overlays, browser proof scripts,
  and renderer/main tests.
- `mods/rundale/scenes.json`: only if a slot/hotspot position needs minor
  adjustment for first-read clarity.
- No backend schema change is expected.

## Data Model

No persistent data-model change. This is a presentation and interaction
milestone using the existing `SceneState.hotspots`, `SceneState.npcs`, and
deterministic activation commands.

## Observable Signal

- Browser screenshot evidence should show no visible `Settings`, `Server`,
  `Connect`, or `Refresh` text on first read.
- Browser transcript should still show scene-click travel through Kilteevan
  Village -> The Crossroads -> Darcy's Pub and Padraig selection.
- Script output should continue to show the same three-scene path and named
  sprite state.

## Feature Flag

No new flag. This is the current visual client behavior, layered on the existing
`diorama` scene-state feature.
