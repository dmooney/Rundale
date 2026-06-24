# visual-direct-action-prompt-m7

This pass makes point-and-click actions feel like the primary way to play.
Hovering or selecting an exit, object, or NPC produces a compact in-game action
prompt (`Go`, `Look`, or `Talk`) anchored in the HUD, while the command input
moves into a small fallback drawer for players who still want to type.

## Affected Subsystems

- `parish/apps/visual/index.html`: add the action prompt and wrap the command
  form in a closed details drawer.
- `parish/apps/visual/src/main.js`: keep selected target state, render action
  prompt copy, trigger the currently hovered/selected target from the prompt,
  and stop reopening/focusing the command input for direct actions.
- `parish/apps/visual/src/styles.css`: style the action prompt as a game HUD
  affordance and keep the fallback drawer compact on desktop/mobile.
- `parish/apps/visual/src/renderer.js`: reuse existing `hotspotCommand` and
  `npcCommand`; no backend schema change expected.

## Data Model

No backend or mod data-model change. M7 uses existing scene state activation
hints and NPC display labels.

## Observable Signal

- Browser proof first-read body text should not include `Type a command` or
  `Send`.
- Hover proof should show action prompt text for travel, inspect, and NPC
  targets.
- Action-button travel proof should move the scene without opening the command
  fallback drawer.
- NPC selection proof should prepare `talk to an older man behind the bar`.

## Feature Flag

No new flag. This is visual-client presentation behavior layered on existing
scene-state support.
