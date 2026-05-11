# parish-npc — Technical Debt

## Open

| ID | Category | Description |
|----|----------|-------------|
| TD-016 | Dead Code | `pub fn build_action_line` in `src/lib.rs:475-488` is only called from its own tests; all production callers (`parish-core/src/ipc/handlers.rs:627`, `src/ticks.rs:351`) use `build_named_action_line(input, None)`. Delete the function and its three local tests, or fold them into `build_named_action_line` tests. |
| TD-017 | Dead Code | `pub async fn extract_intended_references` (`src/lib.rs:592-643`) and `pub fn format_reference_hint` (`src/lib.rs:646-655`) are exported but have zero callers anywhere in the workspace (verified with `rg`). Two-pass dialogue generation was apparently superseded; remove both functions and the private `ReferencePrePassResponse` (`src/lib.rs:580-585`) that only feeds them. |
| TD-018 | Duplication | `ReactionLog::add` (`src/reactions.rs:103-115`) and `add_player_message_reaction` (`src/reactions.rs:122-139`) are identical except for the parameter name; same for `context_string` (`145-165`) vs `npc_context_string` (`171-191`) — same loop, only the format prefix differs. Extract a private `push_entry(&mut self, emoji, ctx, ts)` and `format_lines(&self, n, fmt_fn)` helper. |
| TD-019 | Duplication | Four near-identical tier-filter helpers `tier1_npcs`/`tier2_npcs`/`tier3_npcs`/`tier4_npcs` in `src/manager.rs:314-347`. Replace with a single `npcs_in_tier(&self, tier: CogTier) -> Vec<NpcId>` and either delete the wrappers or keep one-line shims. |
| TD-020 | Duplication | Tier-state management is copy-pasted 3× across tiers in `src/manager.rs:365-476` (`needs_tier{2,3,4}_tick`, `_with_config`, `last_tier{2,3,4}_game_time`, `record_tier{2,3,4}_tick`, `tier{2,3,4}_in_flight`, `set_tier{2,3,4}_in_flight`) — only the tier number, time-unit (minutes/hours/days) and config field differ. Collapse into a `TierTickState` struct with `needs_tick`, `record`, `last_time`, `in_flight`, `set_in_flight` methods, parameterised on the interval. |
| TD-021 | Complexity | `Intelligence::prompt_guidance` in `src/types.rs:94-224` is a 130-line function consisting of six near-identical 5-arm `match` blocks (verbal/analytical/emotional/practical/wisdom/creative). Extract a table-driven helper: e.g., a `&'static [(u8, &str)]` per dimension and a small `score_label(score, table)` lookup. Should shrink to ~25 lines. |
| TD-022 | Complexity | `tier4::apply_events` in `src/tier4.rs:349-522` is a 185-line monolithic `match`. Each arm (`Illness`, `Recovery`, `Death`, `Birth`, `SeasonalShift`, `TradeCompleted`, `FestivalDetected`, `FestivalBond`) is a self-contained handler. Extract one `apply_*` helper per variant and reduce the dispatch loop to a thin `match`. |
| TD-023 | Logic Smell | `tier4::apply_events` Death-without-banshee branch at `src/tier4.rs:412-423`: `npcs.remove(npc_id)` runs unconditionally even when the preceding `if let Some(npc) = npcs.get(npc_id)` failed. If the NPC is missing, `name.unwrap_or_default()` would silently emit `" has passed away."`. Combine into a single `if let Some(npc) = npcs.remove(npc_id) { ... }`. |
| TD-024 | Logic Smell | `tier4::apply_events` Birth arm (`src/tier4.rs:425-441`) uses `unwrap_or_default()` for both parent names; if either parent is missing the description becomes `"A child has been born to  and ."` (empty names, double space). Skip the event or fall back to a sensible label when a parent id can't be resolved. |
| TD-025 | Dead Code (test-only public surface) | Four `pub` no-config shims in `src/ticks.rs` are only called from internal `#[cfg(test)] mod tests` and have no external callers: `relationship_label` (line 72), `build_enhanced_system_prompt` (line 147), `build_enhanced_context` (line 331), `apply_tier1_response` (line 417). Either delete and inline the `_with_config` form into the tests, or downgrade to `pub(crate)` to shrink the leaf-crate API surface. |

## In Progress

*(none)*

## Done

| ID | Category | Description |
|----|----------|-------------|
| TD-001 | Weak Tests | Added 6 tests for death system: multiple simultaneous dooms (herald + death), DOOM_HERALD_WINDOW_HOURS boundary (exactly 12h, 12h+1m), clock rewind (doesn't double-herald, past-doom rewind safe). |
| TD-002 | Duplication | Replaced local `make_test_npc` helpers in `ticks.rs` and `test_npc` in `reactions/arrival_reactions.rs`/`reactions/emoji_reactions.rs` with delegations to `test_helpers::make_test_npc`, eliminating structural field-initialization duplication. |
| TD-010 | Complexity | Split 2,017-line `reactions.rs` into `reactions.rs` (shared palette + ReactionLog, ~300 lines), `reactions/emoji_reactions.rs` (keyword + LLM reactions, ~230 lines), and `reactions/arrival_reactions.rs` (arrival reactions + LLM greeting, ~850 lines). Public API preserved via re-exports. |
| TD-003 | Complexity | Extracted `resolve_cuaird_location()` and `needs_weather_shelter()` from `tick_schedules()` (~130 lines → two focused helpers + slim loop body). |
| TD-004 | Complexity | Extracted `select_reaction_kind()` (kind-selection chain) and `cap_reactions_by_priority()` (truncation pass) from `generate_arrival_reactions()` (~85 lines). |
| TD-005 | Complexity | Extracted 8 context-block helpers (`interlocutor_block`, `other_npcs_block`, `conversation_block`, `continuity_block`, `reactions_block`, `stm_block`, `ltm_block`, `gossip_block`) from `build_enhanced_context_with_config()` (~100 lines). |
| TD-006 | Dead Code | Removed `month_name()` (duplicated `chrono::format("%B")`), replaced call site with `now.format("%B")`. |
| TD-007 | Weak Tests | Added 5 direct unit tests for `find_eligible_couples()`: normal pair, one ill, both outside age range, one in range, duplicate romantic relationships. |
| TD-008 | Weak Tests | Added 4 boundary tests for `pick_next_speaker()`: strength exactly 0.1 (no bonus), just above 0.1 (bonus), negative -0.1001 (abs check), exactly 0.5 threshold (eligible). |
| TD-009 | Weak Tests | Added `dead_npc_excluded_from_birth_check`: brute-force seed search to find a tick where age-100 NPC dies, then verifies no birth involves the dead NPC. |
| TD-012 | Dead Code | Removed unused `MemoryKind::ReceivedGossip` variant. |
| TD-013 | Dead Code | Removed unused `tempfile` from `[dev-dependencies]`. |
| TD-014 | Stale Docs | Updated `CogTier` doc: removed "future" labels from Tier 3 and Tier 4 descriptions. |
| TD-015 | Dead Code | Removed `DailySchedule` struct/impl (superseded by `SeasonalSchedule`), converted 3 tests to use `SeasonalSchedule`. |
| TD-002 (partial) | Duplication | Replaced local `make_npc` helpers in `transitions.rs` and `autonomous.rs` with `test_helpers::make_test_npc`. |
| TD-011 | Complexity | Extracted inline example JSON from `build_tier1_system_prompt()` into `EXAMPLE_RESPONSE_BLOCK` const (~55 → 15 lines in function body). |




