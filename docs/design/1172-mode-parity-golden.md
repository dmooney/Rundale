# Design: Mode-parity golden test + per-turn chokepoint (#1172 + #1173)

Workstream B1 (#1172, golden test) and B2 (#1173, chokepoint extraction) are
designed together because the test's "green across all paths" goal is only
reachable once the refactor removes the headless/harness drift. B1 lands
first as the guard; B2 lands the unification and turns the guard fully green.

## Player/system experience

No player-visible behavior change. The fix is to a recurring **defect class**:
the three per-turn dialogue paths drift silently, and three shipped bugs
(#1028, #1035, #1077/#1079) trace to that drift. After this workstream, all
three paths run the same per-turn cross-cutting steps and emit the same
`GameEvent` stream, mechanically enforced by a golden test.

## The three paths today (verified)

| Step                               | Harness (`testing.rs`) | Headless (`headless.rs`) | Live (`npc_turn.rs`) |
| ---------------------------------- | :--------------------: | :----------------------: | :------------------: |
| `detect_and_record_player_name`    |       yes (1450)       |          **no**          |      yes (107)       |
| `apply_tier1_response_with_config` |       yes (1463)       |        yes (771)         |      yes (330)       |
| `conversation_log.add`             |       yes (1481)       |        yes (783)         |        **no**        |
| `record_witness_memories`          |       yes (1491)       |        yes (793)         |        **no**        |
| publish `DialogueOccurred`         |       yes (1508)       |          **no**          |      yes (352)       |

Three independent divergences, each a latent parity bug:

- headless drops name detection and `DialogueOccurred`;
- the live loop never records `conversation_log` / witness memories inline.

`conversation_log` and witness memories are NOT currently event-driven — no
bus subscriber calls them (subscribers are `CharacterLogManager`,
`LocationLogManager`, `ChatTranscriptLog`, all on-disk logs). So "the live loop
records them via the event bus" is false; the live loop simply doesn't record
them. The chokepoint must do all four steps inline so every path agrees.

## Affected subsystems

- `parish-core` (`parish_core::npc` or `game_session.rs`): owns the new shared
  `apply_npc_turn` chokepoint, parameterized over `EventEmitter` per AGENTS
  rule 12, sitting alongside `apply_movement`.
- `parish-engine` (`testing.rs`, `headless.rs`): replace duplicated bodies with
  a call to the chokepoint.
- `parish-core/src/game_loop/npc_turn.rs`: live loop calls the chokepoint for
  the inline steps (it keeps its async inference + streaming wrapper).
- `parish-engine/tests/mode_parity.rs`: the new golden test.
- `docs/agent/harness.md`: enforcement-status doc update.

## Crate-boundary decision (deviation from issue text)

Both issues suggest `parish-core/tests/mode_parity.rs`. That cannot host the
end-to-end test: `parish-engine` depends on `parish-core` (not the reverse),
so a `parish-core` test cannot reach the harness (`testing.rs`) or headless
(`headless.rs`) paths. **The cross-path golden test must live in
`parish-engine/tests/mode_parity.rs`**, which can reach all three (it owns
harness + headless and depends on core for the live loop). A seam-level
companion unit test MAY also live in `parish-core` exercising the chokepoint
directly over a `CapturingEmitter`, but the authoritative parity assertion is
in `parish-engine`. This is the one open decision flagged for review.

## Observable signal

`GameEvent` is `Clone + PartialEq + Serialize` (`parish-types/src/events.rs`),
so captured streams diff directly. The test captures each path's published
events (via the world `event_bus` / a `CapturingEmitter`), normalizes the two
documented non-deterministic fields (`request_id`, `timestamp`), and asserts
vector equality. A helper renders a readable diff on mismatch naming the path
and the missing/extra variant.

## Shared chokepoint shape (B2)

```rust
// parish-core, alongside apply_movement
#[allow(clippy::too_many_arguments)] // mirrors apply_movement
pub fn apply_npc_turn(
    world: &mut WorldState,
    npc_manager: &mut NpcManager,
    npc_id: NpcId,
    parsed: &NpcStreamResponse,
    player_input: &str,
    game_time: DateTime<Utc>,
    location: LocationId,
    npc_display_name: &str,
    npc_actual_name: &str,
    request_id: Option<u64>,
    config: &NpcConfig,
) -> Vec<String> /* debug events */ {
    // 1. detect_and_record_player_name
    // 2. apply_tier1_response_with_config
    // 3. conversation_log.add(ConversationExchange { .. })
    // 4. record_witness_memories(..)
    // 5. publish DialogueOccurred { request_id, event-time location, .. }
}
```

`request_id: Some(..)` only on the live path is the one documented difference
the parity test normalizes. Publishing goes through `world.event_bus` (the
existing concrete bus) — this fn is backend-agnostic and takes `&mut
WorldState`, matching `apply_movement`'s signature style, so it satisfies rule
12 without needing the `EventEmitter` trait object (the bus is already the
backend-agnostic seam for `GameEvent`). The `EventEmitter` trait is for the
JSON IPC emit layer, which the entry points keep wiring themselves.

## Feature flag

Per AGENTS rule 6 new behavior is flagged. The behavior _added_ is headless +
live now doing the previously-missing steps. Gate the newly-emitted steps
behind `config.flags.is_enabled("turn-chokepoint")` (default-on) so the parity
unification can be toggled off if it surfaces a regression in a shipped path.

## Sequencing / "green" tension (flagged for review)

#1172 asks for the test green now; #1173 asks the same test green "across all
paths" after the refactor. These conflict, because today the paths genuinely
differ. Two options:

- **A (recommended):** Land B1's test asserting harness-vs-headless parity for
  the variants they _already_ share, plus a `#[ignore]`d full-three-way case
  documenting the known divergence with a `// #1173` note. B2 then implements
  the chokepoint, un-`#[ignore]`s the case, and it goes green. Clean history,
  each PR self-consistent.
- **B:** Fold B1+B2 into one PR — write the test and the chokepoint together,
  green from the first commit. Simpler but loses the "test guards the refactor"
  property the issues explicitly want.

Recommendation: **A**, two PRs, #1172 then #1173.
