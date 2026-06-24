# Plan: visual-direct-action-prompt-m7

1. Add closed-by-default command fallback markup and a separate action prompt
   surface to `parish/apps/visual/index.html`.
2. Update `styles.css` so the action prompt is visible, compact, and
   game-like, while the command input is hidden until the fallback drawer is
   opened.
3. Update `main.js` to track the current interactive target, render `Go`,
   `Look`, and `Talk` prompts, and allow the prompt button to activate the same
   target without requiring text input.
4. Add source-level visual regression tests that first-read HTML no longer
   exposes the command input text and that action prompt controls exist.
5. Run visual checks/build and the M7 headless fixture.
6. Capture live browser screenshots/transcript for first read, hotspot hover,
   inspect hover, action-button travel, NPC hover/selection, and mobile first
   read; write evidence and judge.
