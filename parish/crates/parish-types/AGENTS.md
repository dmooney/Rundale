# parish-types — agent scope

Foundational shared types for the Parish engine. Backend-agnostic leaf crate with **zero internal dependencies** — every other Parish crate depends on this one. Defines core types: IDs (entity, location, NPC), time (in-game clock), events (game events stream), conversations (dialogue state), gossip, dice/random, and error types. See root [`AGENTS.md`](../../../AGENTS.md) for non-negotiable rules.

## Scoped commands

```sh
cargo test -p parish-types                        # unit tests
cargo doc  -p parish-types --no-deps --open       # type docs
```

## Local gotchas

- **Zero internal dependencies — transitive impact.** `parish-types` is the dependency root. Never add a dependency on another Parish crate here. Adding types to this crate has workspace-wide implications because every crate depends on them — consider whether a type belongs in a higher leaf crate first.
- **Serialization types are load-bearing.** All types should derive `Serialize`/`Deserialize` where appropriate. Changing serialized field names, types, or semantics may break save compatibility — check `parish-persistence` schema before and after such changes.
- **`#[serde(default)]` for schema evolution.** Use on optional fields to handle forward-compatible deserialization of old save data when new fields are added.
- **`GameTime` is canonical.** `parish_types::time::GameTime` is the in-game clock type used everywhere — game loop, NPC schedules, events, persistence. Do not introduce alternative in-game time representations.
- **Adding `AnachronismEntry` here is load-bearing.** It lives in `lib.rs` directly (not a module) because it is shared between `parish-npc` (detection) and `parish-core` (mod loading). Changes must update both consumers.
- **No runtime-specific code.** As a backend-agnostic leaf crate, must never depend on `tauri`, `axum`, `tower*`, `wry`, or `tao` (enforced by architecture-fitness test).

## Module map

`ids.rs` entity/location/NPC identifier types, `time.rs` in-game clock (`GameTime`, `GameClock`, `GameSpeed`, `Season`, `TimeOfDay`), `events.rs` game event types + `EventBus`, `conversation.rs` dialogue/conversation state (`ConversationExchange`, `ConversationLog`), `gossip.rs` gossip and rumor spreading types (`GossipItem`, `GossipNetwork`), `dice.rs` dice rolling and RNG utilities (`DiceRoll`, `roll_n`, `fixed_n`), `error.rs` thiserror-based error types (`ParishError`).
