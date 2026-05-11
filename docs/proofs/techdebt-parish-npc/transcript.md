Evidence type: gameplay transcript

## Summary

Resolved 13 TODO.md items (with 3 remaining as follow-ups) in parish-npc:

**Tests added (TD-001, TD-007, TD-008, TD-009):**
- Death system: multiple simultaneous dooms (herald + death), DOOM_HERALD_WINDOW_HOURS boundary (12h), clock-rewind safety
- find_eligible_couples: normal pair, ill partner, age range edges, duplicate romantic relationships
- pick_next_speaker: relationship strength exactly 0.1, just above 0.1, negative 0.1001, SPEAK_UP_THRESHOLD 0.5 boundary
- dead_ids exclusion in tick_tier4: brute-force seed search, verify no birth involves dead NPC

**Refactors (TD-003, TD-004, TD-005):**
- tick_schedules: extracted resolve_cuaird_location(), needs_weather_shelter()
- generate_arrival_reactions: extracted select_reaction_kind(), cap_reactions_by_priority()
- build_enhanced_context_with_config: extracted 8 context-block helpers

**Dead code removed (TD-006, TD-012, TD-013, TD-015):**
- month_name() → now.format("%B")
- MemoryKind::ReceivedGossip variant
- tempfile dev-dependency
- DailySchedule struct (converted tests to SeasonalSchedule)

**Docs fixed (TD-014):**
- CogTier: removed "future" from Tier 3/4 descriptions

**Test helper dedup (TD-002 partial):**
- transitions.rs: direct swap (identical defaults)
- autonomous.rs: replaced Npc::new_test_npc() with test_helpers::make_test_npc()

## Verification

```
$ cargo test -p parish-npc
running 400 tests
test result: ok. 400 passed; 0 failed

$ cargo clippy -p parish-npc -- -D warnings
Finished dev profile — no warnings

$ cargo fmt --check
(no output — clean)

$ just agent-check
(passed)

$ just witness-scan
(passed)
```
