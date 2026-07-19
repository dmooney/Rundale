# Issue #1755 — Illustrated Notebook UX Audit

Date: 2026-07-19

## Five Whys

**Observed problem:** activating a sewn-page tab opened a generic sheet, and
Places opened the same full map as the separate Map card.

1. Why? `IllustratedNotebookGame.openTab` translated every tab to a
   `NotebookSurface`; `places` translated directly to `map`.
2. Why? The shared overlay coordinator was the only implemented destination
   model, so both book navigation and transient tools were expressed as overlay
   routes.
3. Why? The interaction slice optimized for proving that every hit target went
   somewhere while the Pixi canvas kept its bounds.
4. Why? Its Playwright contract asserted dialog presence and canvas stability,
   not the meaning promised by a control or its return path.
5. Why? The repository had a detailed visual north star but no required live,
   human-style interaction audit for a player-facing surface.

**Root cause:** notebook sections and transient tools had no distinct product
model, and the verification contract could not detect that semantic collapse.

**Prevention:** root `AGENTS.md` now requires the reusable live audit in
`.github/prompts/rundale-illustrated-notebook-ux-audit.prompt.md`; the focused
tests assert section identity, Places/Map distinction, state preservation, and
return behavior.

## Before/After Model

| Element | Before | Intended and implemented model |
| --- | --- | --- |
| Notes tab | Generic Journal sheet | Current scene notes on the sewn page |
| People tab / portrait | People sheet | Selected-person record on the sewn page |
| Places tab | Full-screen map | Written current/adjacent-place directory on the sewn page |
| Rumours tab | Generic drawer | Learned-story section on the sewn page |
| Journal tab | Generic chat drawer | Recent narrative/conversation entries on the sewn page |
| Map card | Same destination as Places | Dismissible geographic route/orientation sheet |
| Time / Intents / More | Mixed secondary content | Notebook-styled, dismissible task sheets |
| Close / return | Return to canvas only | Restore the invoking control, active tab, scene, draft, and canvas bounds |

## Interaction Inventory

| Visible control | Player-facing result | Return path |
| --- | --- | --- |
| Nearby portrait / scene person | Select person and turn to People record | Choose another tab or person |
| Notes, People, Places, Rumours, Journal tabs | Turn the sewn page to that named section | Turn another tab; no modal close needed |
| Talk, Ask, Help, Observe, Leave | Seed a visible intent draft | Edit or submit the intent |
| Intent strip / quill | Focus or submit the command | Draft remains editable; command result returns to scene |
| Map card | Open geographic route/orientation sheet | Close button; focus returns to Map |
| Time card | Open time/weather sheet | Close, Escape, or clear backdrop |
| Active Intents card | Open current-task sheet | Close, Escape, or clear backdrop |
| More | Open utility sheet for Focail, Save/Load, Debug, Mod, Bug Report, Shortcuts | Close or complete the selected task |

The masthead exposes location/time/weather feedback, the command strip is the
primary action, the five tabs are visible navigation, and **Leave** is a visible
safe route away from the current situation without an undocumented command.

## Final Findings

| Expectation | Evidence | Resolution / deferral |
| --- | --- | --- |
| Tabs are notebook content | `desktop-section-*.png`, `mobile-section-*.png`; semantic section tests | Resolved: five distinct in-page sections with active-tab state |
| Places differs from Map | Places screenshots plus `desktop-map-sheet.png` and `mobile-map-sheet.png` | Resolved: directory versus geographic sheet |
| Understandable open/close/focus loop | Focus restoration and active-section Playwright assertions | Resolved |
| Scene, draft, and canvas survive navigation | Playwright draft, section, and bounds assertions | Resolved |
| Primary action, feedback, navigation, exit visible | Desktop/mobile first-page screenshots and control inventory | Resolved |
| Deliberate sheets share visual language | Map and utility screenshots | Resolved by the existing notebook sheet host; no deferral |

Proof images are generated under `.proofs/1755/` by
`e2e/illustrated-notebook-interactions.spec.ts` and intentionally remain
untracked runtime evidence.
