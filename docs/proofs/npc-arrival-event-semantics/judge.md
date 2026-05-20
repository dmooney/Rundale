# Judge Verdict — npc-arrival-event-semantics

## Summary

This fix correctly addresses the root cause of duplicate NPC arrival entries: the semantic mismatch between `GameEvent::NpcArrived` (intended for physical movement) and the old implementation (publishing on any cognitive-tier transition). The fix eliminates the publish site in `tier_assign.rs` and centralizes all arrival/departure events to the two places where NPCs actually move: `schedule.rs` (schedule-driven transit) and `ticks.rs` (Tier 3 LLM-driven relocations). The supporting dedup state (`last_arrival`, `bump_npc`, etc.) is completely removed from `character_log.rs` and `location_log.rs`, making both managers stateless and enabling straight-through appends.

## Verification of Implementation Claims

**C1 — Schedule transit publishing:** Confirmed. `schedule.rs:187` publishes `NpcDeparted` when `npc.state` transitions to `InTransit` (line 202–206), and `schedule.rs:224` publishes `NpcArrived` when `InTransit` completes and `npc.state` becomes `Present` (line 234). The timing is exact: departure happens when the NPC leaves, arrival when they reach the destination after travel time. No intermediate events.

**C2 — Tier promotion silent:** Confirmed. Grepping the entire codebase finds zero `publish` calls in `tier_assign.rs`. The docstring explicitly states the contract (lines 34–42): "Does **not** publish `NpcArrived` / `NpcDeparted`" and explains why. The unit test `tier_promotion_does_not_fire_npc_arrived` (snapshot.rs:1137–1200) confirms that restoring after a save and promoting an NPC Tier2→Tier1 produces zero `NpcArrived` events (assertions at lines 1196–1200).

**C3 — Tier 3 LLM moves:** Confirmed. `ticks.rs:1191–1210` guards location changes with `if new_loc != npc.location` (preventing phantom arrivals), then publishes `NpcDeparted` from the old location (line 1200–1204) and `NpcArrived` at the new location (line 1205–1209). Both carry the same timestamp, creating a clean before-after pair.

**C4 — Dedup removal:** Confirmed. Grepping for all dedup helper names (`bump_last_arrival`, `bump_npc`, `bump_player`, `last_npc_at`, `last_player_arrival`, `scan_existing_npc_arrivals`, `scan_existing_player_arrival`, `parse_last_arrival_location`) in `character_log.rs` and `location_log.rs` returns zero matches. Both files' `process_event` methods are now stateless: they take the event, extract the necessary IDs/names/paths, call `append_journal_entry`, and return. No filtering, no dedup state, no early returns based on history. The test `writer_appends_every_event_it_receives` (location_log.rs) asserts the new contract: sending `Arrived → Departed → Arrived` produces exactly 2 arrival headings and 1 departure heading (no filtering).

**C5 — Round-trip play-test output:** Confirmed. Sample artifacts show:
- `loc-001-the-crossroads.txt`: 2 player arrivals (outbound + return), 0 NPC entries.
- `loc-002-darcy-s-pub.txt`: 1 player arrival, 0 phantom NPC arrivals (previously showed "Niamh Darcy arrived" + "Padraig Darcy arrived" on every entry due to Tier4→Tier1).
- `loc-015-kilteevan-village.txt`: 4 departures (08:00), 1 player arrival (08:34), 0 duplicate arrivals (pre-fix would show each of the 4 NPCs arriving again on the return trip).
- `npc-019-brigid.txt`: 1 departure from village, 1 arrival at holy well — a clean pair matching the schedule window (08:00 depart, 08:13 arrive with 13-minute travel time).

**C6 — Full test suite:** Confirmed. `cargo test --workspace` = 2858 passed, 15 ignored. `cargo clippy --workspace --all-targets -- -D warnings` = no issues. These are clean green signals.

## Critical Analysis

**Root cause addressed:** Yes. The fix attacks the publish-site semantics, not symptoms. Before: tier transitions fired events, then dedup filtered them. After: only real moves fire events, no dedup needed. This is architecturally sound.

**No escape hatches:** Verified all 4 publish sites (2 in schedule.rs, 2 in ticks.rs). No hidden publishers, no dedup lingering elsewhere.

**Stateless invariant held:** Character and location logs are now pure event recorders. This removes cognitive load at the cost of relying on the publisher (schedule + tier3 path) to emit correctly. That's the right trade-off.

**Test coverage:** The `tier_promotion_does_not_fire_npc_arrived` test directly asserts the fix's core claim: promoting an NPC toward the player must not fire an arrival event. The `writer_appends_every_event_it_receives` test confirms that the log managers are stateless and accept all events without filtering. Both are integration-level tests grounded in the real types.

**Evidence quality:** The live gameplay transcript (round-trip walk) is a strong signal. The sample files show output that matches the expected schema (departures and arrivals in clean pairs, no phantom entries). The heading counts provided in evidence.md (2 arrivals + 0 NPC entries at Crossroads, etc.) are consistent with the sample files.

## Potential Debt

None identified. The removal of dedup state is complete. The new publish sites are minimal and correct. Tier 4 → Tier 1 promotions were the only case where the old code misbehaved; they are now correctly silent.

---

Verdict: sufficient

Technical debt: clear

Acceptance criteria: met
