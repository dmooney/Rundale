# Visual Canvas Fallback Game Read M19

## Player Experience

The Pixi client is still the target renderer, but the canvas fallback should not reveal the scaffolding if it is ever used. A player should see the scene plate and people in the world, then get a subtle cue only for the active or selected target. The first read should not include visible hotspot rectangles, NPC slot IDs, every NPC name, mood emoji, or an overlaid scene title.

## Affected Subsystems

- `parish/apps/visual`: canvas fallback presentation, static regression tests, and a draw-call proof for fallback behavior.
- `parish/testing/fixtures`: deterministic script proving the `/scene` contract still provides the layer/hotspot/slot/NPC data consumed by both renderers.
- `mods/rundale`: no content changes expected.
- Backend crates: no behavior change expected. Existing scene state remains the source of truth.

## Data Model

No schema changes. The fallback continues to consume the same `buildSceneDisplayModel()` output as the Pixi renderer. This milestone changes only canvas drawing policy:

- Inactive hotspots remain hit-testable but invisible.
- Active or selected hotspots draw a small diegetic cue near the target, not a full debug rectangle.
- NPC sprites remain visible.
- NPC captions appear only for active or selected NPCs.
- Slot geometry remains data for placement only, not a visible first-read overlay.

## Observable Signals

- Static regression tests reject the old `drawSlots`, all-hotspot rectangle, unconditional hotspot-label, unconditional NPC-label, mood-emoji, and scene-title paths.
- A fallback proof drives `renderSceneModel()` with a mock canvas context and verifies inactive targets do not emit debug draw calls, while active/selected targets still emit cue calls and captions.
- The live browser smoke proof verifies the Pixi client still starts as the graphical game client and keeps the three-scene slice navigable.

## Feature Flag

No new engine feature flag. This is a visual-client fallback-renderer milestone using already-shipped scene state.
