# Visual Raster Hotspot Cues M17

This slice removes the last visible code-drawn hotspot affordance from the visual client. A player hovering a clickable place should still get immediate feedback, but the feedback should be a small raster PNG sprite overlay, matching the rest of the sprite-composited game screen instead of looking like vector UI drawn over the art.

Affected subsystems:

- `parish/apps/visual`: adds local PNG cue assets and changes `PixiSceneRenderer` hotspot drawing to sprite placement.
- `parish/apps/visual` tests: update regression coverage so travel/inspect cues are no longer code-drawn geometry.
- `.proofs`: live browser proof captures hover screenshots for travel and inspect cues.

Data model:

- No backend schema change. Hotspot hit areas and activation commands stay in `SceneState.hotspots`.
- Cue selection is client-side by hotspot activation kind: travel uses the travel cue sprite, inspect/talk use the inspect cue sprite.

Observable signal:

- Browser proof hovers Kilteevan road, Crossroads pub lane, and Pub hearth/bar hotspots. The transcript records the expected action prompt while screenshots show raster cue sprites without chevrons or corner brackets.

Feature flag:

- None. This is a visual-client rendering refinement with no gameplay state change.
