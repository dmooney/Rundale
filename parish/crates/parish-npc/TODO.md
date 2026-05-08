# parish-npc — Technical Debt

## Open

*(none)*

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




