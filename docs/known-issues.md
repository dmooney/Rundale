# Known Issues

> [Docs Index](index.md)

## Active

*No active issues.*

## Resolved

### 4. NPC conversation memory not wired into LLM prompts
**Severity:** Medium — **Fixed 2026-04-03**
**Description:** Phase 3 added `ShortTermMemory` (20-entry ring buffer) to each NPC, but the memory entries were not included in `build_tier1_context()`. Each LLM interaction was effectively stateless.
**Fix:** `build_enhanced_context_with_config()` in `npc/ticks.rs` now injects recent short-term memories (up to 5), long-term memory recall (keyword-based, up to 3), and gossip context (up to 2) into every Tier 1 prompt.

### 3. Inline separator metadata leaks into NPC dialogue
**Severity:** Medium — **Fixed 2026-03-20**
**Description:** When the LLM puts `---` inline with dialogue instead of on its own line (e.g., `(smiles) --- {"action":...}`), the separator filter failed to detect it, causing JSON metadata to display to the player.
**Fix:** Extended `find_response_separator()` to detect `" --- "` and `" ---\n"` inline patterns in addition to `---` on its own line. Increased `SEPARATOR_HOLDBACK` from 16 to 24 bytes.

### 2. LLM fallback fails for unusual movement verbs
**Severity:** Medium — **Fixed 2026-03-20**
**Description:** Unusual verbs like "saunter", "mosey", "wander" fell through to the LLM which returned `Unknown`.
**Fix:** Expanded `parse_intent_local()` with 36 multi-word "verb to" phrases and 27 single-verb prefixes including saunter, mosey, wander, stroll, amble, trek, hike, sprint, march, creep, sneak, and multi-word phrases like "make my way to", "pop over to", "nip to".
