# Illustrated Notebook UX Audit

Use this prompt before declaring a material change to the illustrated notebook
play surface complete, and whenever a player reports that the interface is
confusing even though its controls technically work.

## Goal

Evaluate and improve the notebook as a coherent game interface. Do not ask the
player to supply a comprehensive requirements list. Infer ordinary interaction
expectations from the visible object, labels, hierarchy, and task at hand; then
make the smallest design changes that make those expectations true.

The notebook is a persistent, diegetic player object. A tab means “turn this
notebook to that section,” not “launch a generic application dialog.” A utility
may use a contained sheet or overlay only when it is genuinely transient or
interruptive. Preserve the player’s context and visual language in either case.

## Required process

1. Read [AGENTS.md](../../AGENTS.md), [LEARNINGS.md](../../LEARNINGS.md), this prompt, the current illustrated
   notebook design documents, and the relevant implementation and Playwright
   tests. Use the Five Whys method before changing code when an unexpected UI
   behaviour is involved.
2. Build and run the real rendered UI. Drive it with pointer/touch-like input
   on desktop and mobile viewports; keyboard and ARIA labels are supporting
   evidence, not a substitute for seeing the surface. Capture a screenshot of
   the first view and every distinct state you inspect.
3. Write an interaction inventory before patching. For every visible control,
   record: label/visual cue, player expectation, resulting state, where content
   renders, how it closes or goes back, and whether player context is retained.
4. State the intended interaction model in a short design note. Name the
   primary work surface, persistent navigation, in-place content, and the few
   justified transient overlays. Make the model implementable and testable.
5. Correct every high-confidence mismatch you find. Do not merely add an
   explanatory tooltip or test an unintuitive result.
6. Re-run the live audit after the changes. Add or revise behavioural and
   Playwright tests so that they assert the semantic result, not only that a
   dialog appeared.

## Non-negotiable review heuristics

- **Object continuity:** content invoked by a notebook tab renders in the
  notebook unless there is a documented, player-visible reason to leave it.
  A new browser-style modal with unrelated chrome, texture, or geometry is not
  an acceptable default.
- **One label, one outcome:** visibly distinct controls must lead to distinct
  player-understandable destinations. For example, **Places** is the notebook’s
  place directory/records and **Map** is geographic orientation/navigation;
  they must not open the same full-map state.
- **Spatial continuity:** opening and closing content preserves the player’s
  scene, selection, scroll/zoom where relevant, focus, and a clear route back.
- **Discoverability:** a player can identify the primary action, navigation,
  state feedback, and safe exit from the first view without having to know
  slash commands, hidden shortcuts, or implementation terminology.
- **Visual continuity:** sheets, drawers, and deliberate overlays inherit the
  notebook’s paper, ink, spacing, interaction language, and responsive
  behaviour. A functional but stylistically unrelated popup fails review.
- **Task coherence:** walk through the normal loops—orient, inspect a person
  or place, act/travel, read the result, resume play, and leave the game. Each
  loop must have an obvious next action and an obvious undo/back/close path.
- **No test-shaped UX:** passing a test that asserts “a control opens an
  overlay” is insufficient. Test and visually verify what appears, where it
  appears, why it is different from adjacent controls, and how the player
  returns.

## Deliverables

Include in the PR or issue:

1. A concise interaction model and inventory (before/after when changing it).
2. Desktop and mobile screenshots or a short recording of the audited flows.
3. A findings table with severity, evidence, resolution, and any deliberate
   deferrals.
4. Tests that distinguish semantic destinations (for example, Places versus
   Map) and retain player context through open/close transitions.
5. A live-play verification note stating what was clicked and observed.

Reject the work as incomplete if an ordinary player would reasonably ask,
“What did that control do, where did my content go, or how do I get back?”
