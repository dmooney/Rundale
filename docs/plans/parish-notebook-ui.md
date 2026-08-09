# Plan: Parish Notebook UI

Status: Retired historical experiment. The `components/notebook` prototype and
its Pixi successor were removed; this file is retained as context only.

## Scope

Build a new root game UI from scratch that matches the Parish Notebook concept.
This is a frontend UI rebuild using existing game state and IPC. It does not
wait for generated exterior art and does not redesign backend gameplay data.

## Milestone 1: Notebook Shell

1. Add `components/notebook/NotebookShell.svelte`.
2. Create child components:
   - `NotebookTopRibbon.svelte`
   - `NotebookWorldStage.svelte`
   - `NotebookNearbyRail.svelte`
   - `NotebookPage.svelte`
   - `NotebookTabs.svelte`
   - `NotebookActionDesk.svelte`
3. Move the root route to render `NotebookShell` for the primary game screen.
4. Keep setup, save picker, debug, demo, bug-report, mod-selector, shortcuts,
   and screenshot overlays wired from the existing route.

## Milestone 2: State and Interaction

1. Add a tiny local selection model for the currently focused NPC.
2. Feed the nearby rail from `$npcsHere`.
3. Feed the ribbon from `$worldState` and existing save/debug stores.
4. Feed the page from the selected NPC or place fallback.
5. Make action stamps focus the input with starters:
   - Talk -> `talk to <name>`
   - Ask -> `ask <name>`
   - Help -> `offer help to <name>`
   - Observe -> `observe <name>`
   - Leave / Map -> open map or focus movement intent.
6. Preserve typed submit behavior via the existing input pipeline.

## Milestone 3: Visual System

1. Add notebook CSS tokens: paper, ink, shadow, tab, stitch, pencil line,
   subdued watercolor washes.
2. Build a parchment top ribbon instead of the old status bar.
3. Build the right notebook page with bound-page/side-tab affordances.
4. Build the bottom desk with stamp buttons and the existing intent field
   visually embedded in parchment.
5. Use the current map/world view as a world-stage placeholder, but frame it
   like the concept rather than the old minimap.

## Milestone 4: Tests and Proof

1. Add focused Vitest coverage for selection/action-starter logic if extracted
   into a helper module.
2. Run `npm --prefix parish/apps/ui run check`.
3. Run the relevant UI tests, at minimum `just ui-test` or focused component
   tests if the full suite is too broad for the iteration.
4. Run a local browser/dev-server proof and capture desktop/mobile screenshots.
5. Run the gameplay fixture `parish/testing/proofs/play_parish-notebook-ui.txt`
   against the live/backend path used for proof.
6. Write `.proofs/parish-notebook-ui/evidence.md` and `judge.md` before the PR
   gate.

## Non-Goals

- No exterior-art generation.
- No new scene schema.
- No new Rust IPC transport.
- No rewrite of NPC memory systems.
- No route-isolated prototype that leaves the old root UI as the real product.

## Expected First Commit

`feat(ui): add parish notebook shell`

This commit should introduce the new component tree and switch the root game
route to it, while keeping the current backend/store plumbing intact.
