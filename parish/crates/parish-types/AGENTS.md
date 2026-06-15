# parish-types — agent scope

Backend-agnostic leaf crate with **zero internal dependencies** — the dependency root that every other Parish crate depends on. Defines core types: IDs, in-game clock, game events, dialogue state, gossip, dice/random, and error types. See root [`AGENTS.md`](../../../AGENTS.md).

## Scoped commands

```sh
cargo test -p parish-types                        # unit tests
cargo doc  -p parish-types --no-deps --open       # type docs
```

## Local gotchas

- **Zero internal deps — workspace-wide impact.** Never add a dependency on another Parish crate. Adding types here affects every crate — consider whether a type belongs in a higher leaf crate first.
- **Serialized field changes break save compatibility.** Check `parish-persistence` schema before renaming, retyping, or reordering serialized fields.
- **`#[serde(default)]` for schema evolution.** Use on optional fields for forward-compatible deserialization of old save data.
- **`GameTime` is the canonical in-game clock.** Used across game loop, NPC schedules, events, and persistence. Do not introduce alternative in-game time types.
- **`AnachronismEntry` lives directly in `lib.rs` (not a module) — load-bearing.** Shared between `parish-npc` (detection) and `parish-core` (mod loading); changes must update both consumers.
- **No runtime-specific code (enforced).** Must never depend on `tauri`, `axum`, `tower*`, `wry`, or `tao`.

## Module map

`ids.rs` entity/location/NPC identifier types, `time.rs` in-game clock (`GameTime`, `GameClock`, `GameSpeed`, `Season`, `TimeOfDay`), `events.rs` game event types + `EventBus`, `conversation.rs` dialogue/conversation state (`ConversationExchange`, `ConversationLog`), `gossip.rs` gossip/rumor types (`GossipItem`, `GossipNetwork`), `dice.rs` dice + RNG utilities (`DiceRoll`, `roll_n`, `fixed_n`), `error.rs` thiserror-based error types (`ParishError`).
