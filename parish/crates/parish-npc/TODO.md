# parish-npc — Technical Debt

## Open

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-001 | Weak Tests | P1 | `src/banshee.rs:1-466` | Death system has no test for multiple simultaneous dooms, exact `DOOM_HERALD_WINDOW_HOURS` boundary (12h), or clock-rewind scenarios. This is critical game logic — a bug here silently loses NPCs or fails to herald deaths. |
| TD-002 | Duplication | P2 | `src/ticks.rs:990-1016`, `src/tier4.rs:534-560`, `src/reactions.rs:1113-1146`, `src/autonomous.rs:101-108`, `src/transitions.rs:194-220` | Five test modules define their own `make_test_npc`/`make_npc`/`test_npc` helpers instead of reusing `test_helpers::make_test_npc` from `lib.rs:1133-1159`. Each builds a full `Npc` struct by hand; differences in field defaults (mood, occupation, age) create subtle test isolation bugs. |
| TD-003 | Complexity | P2 | `src/schedule.rs:71-203` | `tick_schedules()` is ~130 lines. Cuaird resolution (O(n²) friend-location scan), weather shelter overrides, and transit state-machine logic are all inlined. Each concern should be a separate function. |
| TD-004 | Complexity | P2 | `src/reactions.rs:552-637` | `generate_arrival_reactions()` is ~85 lines with a deeply nested reaction-kind selection chain (workplace + introduced + priest + type-roll branches) and a separate truncation-by-priority pass. Split kind selection and capping into helper functions. |
| TD-005 | Complexity | P2 | `src/ticks.rs:158-265` | `build_enhanced_context_with_config()` is ~100 lines sequentially appending 8 context blocks (interlocutor, other NPCs, conversation log, continuity cue, reactions, STM, LTM recall, gossip). Each block could be a private helper returning `Option<String>`. |
| TD-006 | Dead Code | P2 | `src/lib.rs:581-596` | `month_name()` is a 14-line private function that duplicates `chrono::NaiveDateTime::format("%B")`. Used exactly once at line 621. Remove and use `now.format("%B")` directly in `build_tier1_context`. |
| TD-007 | Weak Tests | P2 | `src/tier4.rs:284-333` | `find_eligible_couples()` has no direct unit test — exercised only indirectly through `tick_tier4`. Missing edge cases: one partner ill, both outside 18–45 age range, duplicate romantic relationships to different NPCs. |
| TD-008 | Weak Tests | P2 | `src/autonomous.rs:31-75` | `pick_next_speaker()` not tested at exact `SPEAK_UP_THRESHOLD` (0.5) boundary. `rel.strength.abs() <= 0.1` filter (line 51) has no test — a value of exactly 0.1 could silently fail the condition. |
| TD-009 | Weak Tests | P2 | `src/tier4.rs:205-247` | `tick_tier4()` collects `dead_ids` (line 205) to skip dead NPCs in birth/trade processing (line 252), but no test verifies this exclusion works. A dead NPC could be incorrectly included in a birth event or trade. |
| TD-010 | Complexity | P2 | `src/reactions.rs:1-1973` | `reactions.rs` is the largest file in the crate (1,973 lines) containing three distinct subsystems: emoji reaction palette (lines 1–353), arrival reaction system (lines 354–1100), and LLM greeting/resolution (lines 966–1100). Split into `emoji_reactions.rs`, `arrival_reactions.rs`, and keep the shared palette in `reactions.rs`. |
| TD-011 | Complexity | P3 | `src/lib.rs:330-392` | `build_tier1_system_prompt()` contains a 60-line `format!()` call whose template string includes inline JSON examples and embedded placeholders. Refactoring the template is brittle because placeholders span multiple indentation levels. |
| TD-012 | Dead Code | P3 | `src/memory.rs:32` | `MemoryKind::ReceivedGossip` variant is defined in the enum but never constructed anywhere. `SpokeWithPlayer`, `SpokeWithNpc`, and `OverheardConversation` are all used; this variant appears to be forward-looking dead code. |
| TD-013 | Dead Code | P3 | `Cargo.toml:26` | `tempfile` in `[dev-dependencies]` is unused. No `use tempfile` or `tempfile::` appears in any source file in this crate. |
| TD-014 | Stale Docs | P3 | `src/types.rs:490-504` | `CogTier` doc comment says Tier 3 is "Batch inference (daily, for distant NPCs — future)" and Tier 4 is "Rules engine only (seasonal — future)". Both are fully implemented and operational since at least Phase 4. |
| TD-015 | Dead Code | P3 | `src/types.rs:346-373` | `DailySchedule` struct and its `entry_at`/`location_at` methods are only used by internal tests. `SeasonalSchedule` (line 403) supersedes it — all non-test code uses `SeasonalSchedule::entry_at` and `SeasonalSchedule::location_at`. |

## In Progress

*(none)*

## Done

*(none)*
