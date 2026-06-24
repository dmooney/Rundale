# Visual Clickable World Proof M22

## Player Experience

Players should be able to operate the graphical client by clicking the scene itself. Hovering over a place in the world produces a small raster cue and short caption, clicking exits moves with a transition, clicking props inspects them, and clicking an NPC selects them with a talk action ready. The text command drawer remains available, but the first proof of playability comes from world interaction.

## Affected Subsystems

- `parish/apps/visual`: invisible interaction telemetry, browser proof, static regression tests.
- `parish/testing/fixtures`: deterministic `/scene` fixture for activation hints and NPC sprites.
- Backend crates: no expected schema or route changes; this milestone consumes existing hotspot activation hints.

## Data Model

No backend data-model change. The visual client adds a browser-only telemetry object:

- `window.__rundaleVisualInteraction.hoveredTarget`
- `window.__rundaleVisualInteraction.selectedTarget`
- `window.__rundaleVisualInteraction.prompt`
- `window.__rundaleVisualInteraction.caption`
- `window.__rundaleVisualInteraction.status`
- `window.__rundaleVisualInteraction.location`
- `window.__rundaleVisualInteraction.submittedCommands`
- `window.__rundaleVisualInteraction.events`

The telemetry is for proof automation only and must not create visible debug UI.

## Observable Signals

- Static tests assert the telemetry exists in source, has no visible DOM/debug copy, and records hover/activation/submitted-command events.
- Live browser proof computes authored canvas coordinates from `/api/scene`, uses real pointer events against the Pixi canvas, and verifies telemetry plus screenshots after hover, travel, inspect, and NPC selection.
- Game transcript proves the three-scene slice still exposes activation hints and NPC sprite data.

## Feature Flag

No engine feature flag. This is visual-client proof instrumentation around existing client interactions and scene activation hints.
