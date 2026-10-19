# limerick-npc — agent scope

NPC simulation: tier promotion/demotion, memory, schedules, reactions, mood. Backend-agnostic — consumed by every entry point. See root [`AGENTS.md`](../../../AGENTS.md).

## Scoped commands

```sh
cargo test -p limerick-npc                            # unit + integration
cargo test -p limerick-npc -- --nocapture transitions # tier state machine
```

Integration tests reference `../../testing/fixtures/...` and `../../../mods/rundale/...` — cwd = crate root.

## Local gotchas

- **`ScheduleEntry::start_hour` means "depart at", not "arrive at".** NPCs are in transit during the first `travel_minutes` of each window. Tests asserting presence at exact `start_hour` will spuriously fail.
- **Tier transitions are async + bounded.** `manager` orchestrates promotions/demotions across tiers (1→4); skipping a tier requires explicit justification. Tier 4 = low-fidelity rules-only.
- **Memory representation has short + long-term split** (`memory/`); long-term writes are persisted via `limerick-persistence` — never write directly.
- **Anachronism subsystem watches dialogue output**, not just NPC state — touches `reactions/`, `overhear/`, `mood/`, `anachronism/`.
- **Schema types are re-exported** for the Limerick Designer + mod tooling. Renames break editor compile.
- **`snapshot.restore` wipes `tier_assignments`** — fixed by calling `npc_manager.seed_tier_state(world)` at the end of `restore`. The seed function is in `limerick-npc/src/tier_assign.rs` and shares its BFS distance compute with `assign_tiers` but publishes no events. Tier is derivable from `(player_location, npc.location, npc.state)` so the snapshot file itself doesn't need to carry it.
- **Broad NPC trait cue detectors need negated-example tests.** Bios may compare against someone else's trait ("lacks his father's cunning"), so matching a bare cue like `cunning` can misclassify the child/foil. Prefer narrower phrases (`cunning with`) and add a negative fixture alongside the positive one.
- **Async Tier-2 results need immutable apply anchors and lineage.** Location/activity equality alone misses restore and A→B→A; carry a transient monotonic participant revision and validate it at one public apply seam before any side effect.
- **Tier-2 activity grounding must identify authored interval instances.** Fingerprint schedule variant/entry and game date so a bulk wait cannot skip B and land on text-identical A.
- **Prompt-roster order is a local-inference performance contract.** `NpcManager::known_roster` formerly leaked randomized `HashSet` iteration into `PEOPLE YOU KNOW`, producing 166 system-prompt variants for seven NPCs and invalidating vllm-mlx prefix reuse halfway through each prompt. Sort stable identities before prompt assembly; on Qwen2.5-14B this changed repeated-turn TTFT from roughly 5–10s to 0.87–1.16s while leaving the authored prompt content unchanged.
- **Reaction history is directional durable gameplay state.** Record player-to-NPC and NPC-to-player observations through the shared directional apply and exact-journal seams, render prompts from the stored direction, and default a missing legacy entry direction to player-to-NPC for snapshot compatibility.
- **Arrival greetings are off by default and dice-driven.** `npc-arrival-greetings` is default-off, and reaction chance/kind roll from a seed of game minute and location (`dice::seeded_n`); a deterministic test needs the flag on and only unmet NPCs at their own indoor workplace at the destination (threshold clamps to 1.0, kind is a model-written introduction).

## Module map

`manager/` tier orchestration, `autonomous/`+`ticks/` sim loops, `memory/` short+long, `transitions/`+`tier4/` cognition, `reactions/`+`overhear/`+`mood/`+`anachronism/` social.
