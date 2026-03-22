# Known Issues

> [Docs Index](index.md)

## Active

### 4. NPC conversation memory not wired into LLM prompts
**Severity:** Medium
**Description:** Phase 3 added `ShortTermMemory` (20-entry ring buffer) to each NPC, but the memory entries are not yet included in `build_tier1_context()`. Each LLM interaction is effectively stateless — the NPC has no context about prior exchanges.
**Expected:** `build_tier1_context()` should inject recent memory entries into the prompt so NPCs recall earlier conversation.

## Resolved

### 3. Inline separator metadata leaks into NPC dialogue
**Severity:** Medium — **Fixed 2026-03-20**
**Description:** When the LLM puts `---` inline with dialogue instead of on its own line (e.g., `(smiles) --- {"action":...}`), the separator filter failed to detect it, causing JSON metadata to display to the player.
**Fix:** Extended `find_response_separator()` to detect `" --- "` and `" ---\n"` inline patterns in addition to `---` on its own line. Increased `SEPARATOR_HOLDBACK` from 16 to 24 bytes.

### 1. TUI does not scroll back
**Severity:** High — **Fixed 2026-03-20**
**Description:** When the main text panel fills up with content, there was no way to scroll back to see earlier output.
**Fix:** Added `ScrollState` to App. Page Up/Down scroll by 10 lines, Up/Down by 1, Home/End jump to top/bottom. Auto-scroll follows new output unless the user scrolls up. Scroll position indicator shown in top-right corner.

### 2. LLM fallback fails for unusual movement verbs
**Severity:** Medium — **Fixed 2026-03-20**
**Description:** Unusual verbs like "saunter", "mosey", "wander" fell through to the LLM which returned `Unknown`.
**Fix:** Expanded `parse_intent_local()` with 36 multi-word "verb to" phrases and 27 single-verb prefixes including saunter, mosey, wander, stroll, amble, trek, hike, sprint, march, creep, sneak, and multi-word phrases like "make my way to", "pop over to", "nip to".
