# Plan: Visual Crossroads/Pub Compositor M4

1. Generate project-local raster PNG atoms from the existing Crossroads and Pub
   pixel plates:
   - muted full-stage base layers,
   - transparent full-stage landmark/prop restoration layers,
   - transparent lighting, glow, shadow, and dampness effect layers.

2. Update `mods/rundale/scenes.json`:
   - add Crossroads atom assets,
   - add Pub atom assets,
   - replace each scene's single `pixel-plate` layer with an ordered atom stack,
   - keep hotspot shapes and NPC slots unchanged.

3. Update tests:
   - extend `parish-mod` real Rundale scene tests to require Crossroads and Pub
     multi-layer PNG atom stacks and reject a lone live `pixel-plate` layer,
   - extend the server scene route test to verify `/api/scene-state` exposes
     Crossroads and Pub atom URLs.

4. Verify the app:
   - run visual `check`, `test`, and `build`,
   - run targeted Rust tests,
   - run the M4 script fixture,
   - run a live visual client against a live backend and capture desktop,
     mobile, and click-through proof artifacts.

5. Complete proof:
   - write `.proofs/visual-crossroads-pub-compositor-m4/evidence.md`,
   - write `.proofs/visual-crossroads-pub-compositor-m4/judge.md`,
   - run `agent-check`.
