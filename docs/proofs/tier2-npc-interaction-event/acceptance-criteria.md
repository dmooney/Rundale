# Acceptance Criteria — tier2-npc-interaction-event

Tier 2 produces narrative `summary` text describing what happened
between a group of NPCs (LLM-generated). The summary currently
lands in each participant's `npc.memory` but never reaches the
event bus, so per-location and per-character log writers can't
record the actual story beat — only mechanical mood/relationship
deltas.

## Criteria

- **C1.** New `GameEvent::NpcInteraction { participants:
  Vec<NpcId>, location: LocationId, summary: String, timestamp:
  DateTime<Utc> }` defined in `parish-types/src/events.rs`.
  `event_type()` returns `"NpcInteraction"`. Serializes with
  tagged-enum shape like the other variants.
- **C2.** `apply_tier2_event_with_config` publishes one
  `NpcInteraction` per `Tier2Event` whose `summary` is non-empty,
  before applying mood / relationship deltas. Empty-summary events
  produce no `NpcInteraction`.
- **C3.** `location_log::process_event` appends to the shared
  location's file under heading `Interaction — <N participants>`
  with body listing participant names and the summary verbatim.
- **C4.** `character_log::process_event` appends one journal entry
  per participant in their own log under heading `Interaction`,
  with body `*With <other-participants>: <summary>*`. Self is
  excluded from the "with" list.
- **C5.** `transitions::event_involves_npc` and
  `transitions::summarize_event_for_npc` updated so the new variant
  participates in tier-promotion context inflation.
- **C6.** `journal_bridge` and `debug_snapshot` accept the new
  variant without warnings.
- **C7.** Live `just demo` (≥30 turns, ~1 hour game time) produces
  at least one `Interaction` entry in a per-location log with a
  non-empty summary.
- **C8.** `cargo test --workspace`, `cargo clippy --all-targets -- -D
  warnings`, `just check` all pass.

## Verification

Same harness as `tier2-events-on-bus` — extend the demo run, grep
for `— Interaction` in the produced location and npc log files.
