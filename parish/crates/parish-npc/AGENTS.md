# parish-npc — agent scope

NPC simulation: tier promotion/demotion, memory, schedules, reactions, mood. Backend-agnostic — consumed by every entry point. See root [`AGENTS.md`](../../../AGENTS.md).

## Scoped commands

```sh
cargo test -p parish-npc                            # unit + integration
cargo test -p parish-npc -- --nocapture transitions # tier state machine
```

Integration tests reference `../../testing/fixtures/...` and `../../../mods/rundale/...` — cwd = crate root.

## Local gotchas

- **`ScheduleEntry::start_hour` means "depart at", not "arrive at".** NPCs are in transit during the first `travel_minutes` of each window. Tests asserting presence at exact `start_hour` will spuriously fail.
- **Tier transitions are async + bounded.** `manager` orchestrates promotions/demotions across tiers (1→4); skipping a tier requires explicit justification. Tier 4 = low-fidelity rules-only.
- **Memory representation has short + long-term split** (`memory/`); long-term writes are persisted via `parish-persistence` — never write directly.
- **Anachronism subsystem watches dialogue output**, not just NPC state — touches `reactions/`, `overhear/`, `mood/`, `anachronism/`.
- **Schema types are re-exported** for the Parish Designer + mod tooling. Renames break editor compile.

## Module map

`manager/` tier orchestration, `autonomous/`+`ticks/` sim loops, `memory/` short+long, `transitions/`+`tier4/` cognition, `reactions/`+`overhear/`+`mood/`+`anachronism/` social.
