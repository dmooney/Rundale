# Plan: npc-arrival-greetings flag

One logical change, one commit: `feat(npc): gate spontaneous arrival greetings
behind npc-arrival-greetings flag (default off)`.

## Steps

1. **Gate `apply_arrival_reactions`** (`parish-core/src/game_session.rs:689`).

   - Add a flag check at the top: if
     `!config.flags.is_enabled("npc-arrival-greetings")`, return `Vec::new()`
     immediately (before NPC lookup / generation / logging). `ReactionConfig` is
     already passed in; confirm it can reach the flag set, otherwise thread the
     `FeatureFlags` (or a `bool`) into the call. Returning empty means downstream
     `effects.arrival_reactions` is empty, so `movement.rs` streaming is a no-op
     in every runtime — no per-entry-point edit needed.
   - Name the flag constant once (e.g. `pub const NPC_ARRIVAL_GREETINGS_FLAG:
&str = "npc-arrival-greetings";`) and reference it from the check, matching
     the existing `AUTONOMOUS_NPC_CHAIN_FLAG` / `SERIALIZE_TURN_STREAM_FLAG`
     pattern in `npc_turn.rs`.

2. **Verify the wiring caller has the flag.** `apply_arrival_reactions` is called
   from `game_session.rs:194` (movement pipeline) and as a standalone. Both have
   access to config. If the signature needs the flag value, pass it from the
   caller; keep the public signature change minimal and update both call sites +
   any tests.

3. **Tests** (`parish-core/src/game_session.rs` test module — there are already
   `apply_arrival_reactions_*` unit tests):

   - Add `arrival_reactions_muted_when_flag_off`: default flags → returns empty,
     `world.text_log` gains no greeting line.
   - Add `arrival_reactions_emitted_when_flag_enabled`: flags with
     `npc-arrival-greetings` enabled → returns ≥1 reaction at a populated
     location (mirrors the existing
     `apply_arrival_reactions_standalone_produces_reactions`).
   - Keep `apply_arrival_reactions_marks_introductions` passing only in the
     enabled case; assert introductions still occur via the dialogue path is
     covered by existing handler tests (no new test needed there).

4. **Docs**: add the flag to the feature-flag list / README flag table if one
   exists; note default-off and the opt-in command in the PR body (rule #6/#7).

## Verification

- `just check` (fmt + clippy + `cargo test -p parish-core`) green.
- Headless live transcript:
  `cargo run --manifest-path parish/Cargo.toml -p parish-cli -- --script parish/testing/proofs/play_npc-arrival-greetings.txt`
  → capture to `.proofs/npc-arrival-greetings/transcript.txt`, map AC1–AC5.
- Tauri spot-check over `:3030`: new-game, move into The Crossroads → no greeting
  at default; `/flag enable npc-arrival-greetings`, move again → greetings return.
- `evidence.md` (Evidence type: live gameplay transcript) + `judge.md` (three
  header lines) + `just agent-check` + `just attach-proof npc-arrival-greetings`.
