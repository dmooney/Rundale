# Acceptance Criteria: Fix NpcArrived/NpcDeparted Dedup Key (#1013)

## Problem

Currently, both `NpcArrived` and `NpcDeparted` events use the same `bump_last_arrival` method
to check if a duplicate entry should be skipped. This means the dedup key is location-only
and shared across event types.

**Symptom:** When an NPC arrives at location A then departs from location A, only the arrival
is recorded. The departure is incorrectly suppressed because it appears to be a duplicate of
the arrival (same location).

## Observable Acceptance Criteria

1. **Separate dedup state for arrivals and departures**
   - Add `last_departure: Mutex<HashMap<NpcId, String>>` to `CharacterLogManager`
   - Implement `bump_last_departure` method mirroring `bump_last_arrival`
   - Call `bump_last_departure` in the `NpcDeparted` event handler instead of `bump_last_arrival`

2. **Arrival-then-departure produces two journal entries**
   - When an NPC has `NpcArrived(loc=A)` followed by `NpcDeparted(loc=A)` in the same session,
     both events produce separate journal entries

3. **Arrival dedup still works**
   - When an NPC has `NpcArrived(A)` then `NpcArrived(A)` (duplicate arrival), only one entry
     is recorded

4. **Departure dedup works**
   - When an NPC has `NpcDeparted(A)` then `NpcDeparted(A)` (duplicate departure), only one
     entry is recorded

5. **Unit test coverage**
   - Add a test covering the arrived-then-departed-from-same-location scenario
   - Verify both entries appear in the log

6. **All cargo tests pass**
   - `cargo test -p parish-core` must pass with green results
