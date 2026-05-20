# Acceptance Criteria — npc-arrival-event-semantics

`GameEvent::NpcArrived` and `GameEvent::NpcDeparted` must describe
**physical movement**, not cognitive-tier transitions. Currently
`tier_assign.rs` republishes `NpcArrived` every time an NPC re-enters
Tier 1, producing duplicate journal entries whenever the player
moves back and forth. The fix is to make the publish site match the
semantic, then delete the dedup workarounds in both log managers.

## Criteria

- **C1.** `NpcArrived` and `NpcDeparted` are published from
  `schedule::tick_schedules` at the same site that pushes the
  corresponding `ScheduleEvent::Arrived` / `Departed` — i.e. when
  the NPC's `state` actually transitions to/from `InTransit`.
- **C2.** `tier_assign::assign_tiers` no longer publishes
  `NpcArrived` on Tier-1 promotion. Cognitive tier changes are
  invisible to the event bus.
- **C3.** Tier-3 LLM-driven location changes (`ticks.rs::1190`
  `npc.location = new_loc`) publish `NpcDeparted` from the previous
  location and `NpcArrived` at the new location.
- **C4.** With the new publish sites, the dedup state and bump
  helpers are deleted from `character_log.rs` and `location_log.rs`
  (`last_arrival`, `last_player_arrival`, `last_npc_at`,
  `last_player_at`, `bump_last_arrival`, `bump_last_player_arrival`,
  `bump_npc`, `bump_player`, `scan_existing_npc_arrivals`,
  `scan_existing_player_arrival`, `parse_last_arrival_location`).
  Every `process_event` branch becomes a straight append.
- **C5.** Live play-test: player walks from village → crossroads →
  pub → crossroads → village (round trip past the same NPC). The
  NPC log shows at most one "Arrived at <village>" heading; the
  village location log shows at most one "<NPC name> arrived"
  heading. No duplicates, no rapid-fire tier-recompute flood.
- **C6.** `cargo test --workspace` passes. `cargo clippy --workspace
  --all-targets -- -D warnings` clean. `just check` passes.

## Verification fixture

`parish/testing/fixtures/play_npc-arrival-event-semantics.txt` —
round-trip walk that previously generated duplicates; now must
produce one entry per real location change.
