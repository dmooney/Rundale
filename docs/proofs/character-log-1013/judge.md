# Judge Verdict: character-log-1013

Verdict: sufficient

Technical debt: clear

Acceptance criteria: met

---

### Detailed Assessment

The fix properly addresses the root cause of issue #1013 by introducing separate dedup state for NPC departure events, independent from arrival dedup state.

**Key Evidence:**

1. **Bug Root Cause Fixed**
   - Previous code: `NpcDeparted` handler called `bump_last_arrival()` (line 346)
   - Fixed code: `NpcDeparted` handler calls `bump_last_departure()` (line 379)
   - Result: Arrivals and departures no longer collide in dedup logic

2. **Implementation Quality**
   - New `last_departure` field properly initialized in both `new()` and `new_at_dir()`
   - `bump_last_departure()` mirrors `bump_last_arrival()` exactly
   - Cross-session seeding handled via `scan_existing_npc_departures()`
   - Parsing logic properly separated into `parse_last_departure_location()`

3. **Test Coverage**
   - Unit test `arrived_then_departed_produces_two_entries()` directly validates the fix
   - Regression tests for arrival and departure dedup independently
   - All tests pass with no existing test failures
   - Test coverage spans normal case, edge cases, and cross-session behavior

4. **Scope Compliance**
   - Strict scope lock respected: only `character_log.rs` modified
   - All required proof files created
   - No extraneous changes

The fix is minimal, surgical, and directly addresses the bug without introducing new dependencies, complexity, or risk.
