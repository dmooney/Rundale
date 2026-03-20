# Known Issues

> [Docs Index](index.md)

## Active

(none currently)

## Resolved

### 1. TUI does not scroll back
**Severity:** High — **Fixed 2026-03-20**
**Description:** When the main text panel fills up with content, there was no way to scroll back to see earlier output.
**Fix:** Added `ScrollState` to App. Page Up/Down scroll by 10 lines, Up/Down by 1, Home/End jump to top/bottom. Auto-scroll follows new output unless the user scrolls up. Scroll position indicator shown in top-right corner.

### 2. LLM fallback fails for unusual movement verbs
**Severity:** Medium — **Fixed 2026-03-20**
**Description:** Unusual verbs like "saunter", "mosey", "wander" fell through to the LLM which returned `Unknown`.
**Fix:** Expanded `parse_intent_local()` with 36 multi-word "verb to" phrases and 27 single-verb prefixes including saunter, mosey, wander, stroll, amble, trek, hike, sprint, march, creep, sneak, and multi-word phrases like "make my way to", "pop over to", "nip to".
