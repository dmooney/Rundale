# Acceptance Criteria — tier2-events-on-bus

Tier 2 simulation runs every 5 game-minutes and produces `Tier2Event`s
carrying `mood_changes` and `relationship_changes`. The current
`apply_tier2_event_with_config` mutates `npc.mood` / `npc.relationships`
directly without publishing the corresponding `GameEvent::MoodChanged`
or `GameEvent::RelationshipChanged` on the event bus. As a result,
character and location log writers — which subscribe to the bus only —
never see Tier 2 background simulation.

## Criteria

- **C1.** `apply_tier2_event_with_config` accepts an `event_bus:
  &EventBus` argument and, for every `MoodChange` where the NPC's
  mood actually changes (`npc.mood != mc.new_mood`), publishes
  `GameEvent::MoodChanged { npc_id, new_mood, timestamp }`.
- **C2.** For every `RelationshipChange` applied, publishes
  `GameEvent::RelationshipChanged { npc_a: from, npc_b: to, delta,
  timestamp }`.
- **C3.** Existing callers in `parish-cli/src/headless.rs` and
  `parish-tauri/src/setup.rs` pass `&world.event_bus` / `&app.world.event_bus`.
- **C4.** No spurious publishes when the mood string is unchanged or
  delta is exactly 0 — only real updates fire events.
- **C5.** Live `just demo` run with the LLM provider exercises Tier 2
  at least once, producing `*<NPC>: mood shifted to <m>*` and/or
  `*<NPC> ↔ <NPC>: bond shifted by …*` entries in the appropriate
  per-location and per-NPC markdown logs.
- **C6.** `cargo test --workspace`, `cargo clippy --workspace
  --all-targets -- -D warnings`, `just check` all pass.

## Verification

Same fixture as the npc-arrival semantics proof — extend a demo run
long enough that Tier 2 fires at least once. Tier 2 cadence
(`tier2_tick_interval_minutes` default: 5 game-min) means 30 demo
turns over ~1 game-hour gives ~12 opportunities.
