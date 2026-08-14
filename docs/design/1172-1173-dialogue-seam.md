# Design: unify the per-turn dialogue chokepoint (#1172 / #1173)

> Status: Proposed · Task: `1172-1173-dialogue-seam` · Closes #1172, #1173

## Problem

The "apply a parsed NPC dialogue response" step is reimplemented in four places
that have silently drifted. Each does a different subset of the five
cross-cutting steps:

| step                                             | live `game_loop::npc_turn::run_npc_turn` | headless `apply_npc_response` | harness `consume_canned_npc_response` | harness `handle_npc_interaction_for` |
| ------------------------------------------------ | ---------------------------------------- | ----------------------------- | ------------------------------------- | ------------------------------------ |
| name detection (`detect_and_record_player_name`) | yes                                      | yes¹                          | yes                                   | yes                                  |
| `apply_tier1_response_with_config`               | yes                                      | yes                           | yes                                   | yes                                  |
| `conversation_log.add(ConversationExchange)`     | **no**                                   | yes                           | yes                                   | **no**                               |
| `record_witness_memories`                        | **no**                                   | yes                           | yes                                   | **no**                               |
| publish `GameEvent::DialogueOccurred`            | yes                                      | **no**                        | yes                                   | **no**                               |

¹ headless detects the name separately, upstream of `apply_npc_response`.

Consequences, all already filed: the live (server/Tauri) path never populates
`conversation_log` (which feeds the "What's been said here" prompt block in
`ticks::conversation_block`) and never records witness memories, so live NPCs
lose scene continuity (latent). Headless never publishes `DialogueOccurred`, so
its journal/location logs miss dialogue (#1035-class). The harness addressed
path (`talk to X`) drops three of the five steps. #1028, #1035, #1077/#1079 all
trace here.

## Change

### Explicit current-turn obligations (#1832)

The pre-inference `DialogueGroundingSnapshot` also carries a conservative,
ordered set of obligations derived from the player's exact live utterance:
known-person referral, stated player name, request for work, and request for
lodging. The same typed values render the `PLAYER REQUESTS TO ANSWER NOW`
prompt contract and validate the final delivered line. This covers declarative
multi-facet introductions that contain no question mark and therefore do not
activate the older answer-first question heuristic.

After semantic guards, repetition handling, and the display cap, the canonical
apply seam verifies that every recognized facet remains acknowledged. A partial
candidate is replaced whole and its metadata discarded before state, memory,
events, or UI output. The deterministic replacement addresses each facet in
player order while making no claim that an NPC is hiring or that a place offers
lodging. Unrecognized or merely topical mentions create no obligation.

### Wave 2: typed factual grounding

Dialogue candidates now cross a semantic trust boundary at the same canonical
apply seam. Before inference, `DialogueGroundingSnapshot` freezes the authored
calendar, people, occupations, workplaces, current locations, and location
relationships that the candidate is allowed to claim. It also carries a small
typed referent context maintained per conversation and cleared on a location
change. The context distinguishes unknown people from role-marked unknown
places and permits pronoun continuity only when the referent is unambiguous.

`validate_dialogue_candidate` compares factual claims against that immutable
snapshot. Unsupported current-festival claims, confirmations of unknown people
or places, and contradictions about occupation, workplace, or geography reject
the entire candidate. Rejection returns the deterministic safe fallback and
discards all candidate metadata before memory, events, NPC state, or UI-visible
events can observe it. This is deliberately a whole-candidate decision: the
validator never substitutes a noun while leaving the surrounding false claim
intact. The direct apply API and both game-loop modes use the same validator and
snapshot, with mode-parity coverage at the real-loop seam.

### B2 (#1173) — one shared seam in `parish-core`

Add `parish_core::game_session::apply_npc_dialogue_turn`, next to
`apply_movement`, doing all five steps over plain `&mut WorldState` /
`&mut NpcManager` borrows (no runtime-specific I/O, so no `EventEmitter`
parameter — the `DialogueOccurred` publish goes to `world.event_bus`, the
GameEvent bus, not the UI emitter). It returns the `Vec<String>` debug-event
strings that `apply_tier1_response_with_config` + `record_witness_memories`
produce, so each caller forwards them exactly as it does today:

```rust
pub fn apply_npc_dialogue_turn(
    world: &mut WorldState,
    npc_manager: &mut NpcManager,
    speaker_id: NpcId,
    parsed: &NpcStreamResponse,
    player_input: &str,          // raw player utterance for name-detect + memory
    player_said_for_journal: &str, // verbatim line for DialogueOccurred.player_said
    game_time: DateTime<Utc>,
    location: LocationId,
    speaker_display_name: &str,
    speaker_actual_name: &str,
    request_id: Option<u64>,
) -> Vec<String>
```

The function:

1. `detect_and_record_player_name(world, npc_manager, player_input, speaker_id)`
2. computes `player_name` via `knows_player_name`, then
   `apply_tier1_response_with_config(npc, parsed, player_input, …)` (collect
   debug events)
3. `world.conversation_log.add(ConversationExchange{…})`
4. `record_witness_memories(npc_manager.npcs_mut(), …)` (collect debug events)
5. `world.event_bus.publish(GameEvent::DialogueOccurred{…})` — gated on
   "either the player line or the dialogue is non-empty", matching the live
   loop's existing guard.

Callers become thin:

- **live** `run_npc_turn` — replace the inline name-detect (already at the top
  of the turn) + tier1 + DialogueOccurred block with one call; it keeps
  ignoring the returned debug events (`let _ = …`), but now also gets
  conversation_log + witness for free. Name detection currently happens in
  `prepare_npc_conversation_turn`'s setup scope; the call site is reorganized so
  detection still precedes prompt build (no behavior change there) and the
  apply-step runs through the seam.
- **headless** `apply_npc_response` — replace its body with the seam call,
  forwarding the returned debug events through `app.debug_event`. The upstream
  `detect_player_name` in `handle_headless_game_input` is **kept**: it seeds
  `world.player_name` before the prompt is built so _this_ turn's prompt can use
  the name. The seam then re-runs `detect_and_record_player_name`, which is
  idempotent (it only sets `player_name` when unset and `teach_player_name` is a
  set-insert), so the double call is harmless — the same pattern the live loop
  uses at `npc_turn.rs` (pre-inference seed + post-response canonical apply).
- **harness** `consume_canned_npc_response` and `handle_npc_interaction_for` —
  both build the synthetic `NpcStreamResponse`, then call the seam and forward
  debug events. The duplicated bodies are deleted (C4).

### B1 (#1172) — mode-parity golden test

`parish-engine/tests/mode_parity.rs`: build a `GameTestHarness`, subscribe to
`world.event_bus`, drive one deterministic dialogue input (`talk to <npc> …`)
through the legacy `execute` path; capture the `GameEvent`s. Roll back to the
pre-state, drive the _same_ input through `execute_via_real_loop` (the real
`game_loop`, MockClient scripted to the same dialogue); capture its
`GameEvent`s. Normalize (strip incidental `timestamp` / `request_id`, sort
set-semantic runs — reuse `shadow::normalize` shape) and assert equality. A
second test drops a step from one path behind a test-only switch (or asserts the
pre-fix inequality on a captured fixture) to prove the guard bites (C6).

Test lives in `parish-engine` (not `parish-core` as the issue's `e.g.`
suggested) because only `parish-engine` can reach all of `GameTestHarness`,
`execute_via_real_loop`, and the headless `App` — `parish-core` cannot depend on
`parish-engine`. Noted as an intentional deviation.

## Affected subsystems

- `parish-core` (`game_session.rs`) — new seam; `game_loop/npc_turn.rs` — call site.
- `parish-engine` (`headless.rs`, `testing.rs`) — route through seam; new
  `tests/mode_parity.rs`.
- No new mod files, no new `Npc`/`World` fields, no new event variants.

## Risks / parity nuances

- **Live path gains behavior** (conversation_log + witness). Confirmed in scope.
  Proven by the golden test (live stream now == harness stream) and by the unit
  suites for those two functions staying green.
- **`teach_player_name` vs `detect_and_record_player_name`.** Headless currently
  calls `detect_player_name` then `teach_player_name(setup.npc_id)` separately;
  the harness `@mention` path and live path use
  `detect_and_record_player_name`, which already teaches the addressed speaker.
  The seam standardizes on `detect_and_record_player_name`; verify the headless
  name proof (`play_f20-harness-player-name.txt`) still passes.
- **Double-publish guard.** No event-bus subscriber records witness memories or
  conversation_log, so adding them inline does not double-count. `DialogueOccurred`
  is published exactly once per turn by the seam; callers must not also publish.
- **Feature flag.** Per AGENTS §6 this is a behavior change to a shared path.
  It is a parity _consolidation_ (no new user-facing feature surface), so it
  rides the existing dialogue path rather than a new flag — called out here for
  review; add `dialogue-seam-unified` default-on gate if reviewers prefer.

## Observable signal (harness)

`talk to Padraig Darcy …` at Darcy's Pub (Padraig + Niamh co-located):
`/debug memory Niamh Darcy` shows `Overheard: a newcomer said '…' and Padraig
Darcy replied '…'` — impossible before the fix because the addressed path never
called `record_witness_memories`. Fixture:
`parish/testing/proofs/play_1172-1173-dialogue-seam.txt`.
