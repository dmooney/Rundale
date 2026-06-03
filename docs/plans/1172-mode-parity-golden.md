# Plan: #1172 (golden test) then #1173 (chokepoint)

Two PRs. Recommendation A from the design note: #1172 lands the guard, #1173
lands the unification and turns the guard fully green.

## PR 1 — #1172 mode-parity golden test

1. `test(parity): capture helper for per-path GameEvent streams`
   - Add `parish-engine/tests/mode_parity.rs`.
   - Helper: build an `App` with the Rundale mod + `SimulatorClient`, move the
     player co-located with one NPC, run one canned exchange, drain
     `world.event_bus` into `Vec<GameEvent>`. Factor a `normalize()` that
     blanks `request_id` and `timestamp` on `DialogueOccurred`.
   - Drive the harness path via `GameTestHarness` + `add_canned_response`.
   - Drive the headless path via the `apply_npc_response` entry (expose a
     `pub(crate)` test seam if needed).
2. `test(parity): assert harness == headless event stream + diff message`
   - Assert normalized stream equality for the variants both share today.
   - Custom assert that prints `path X missing/extra: <variant>` on mismatch
     (C3 message quality).
   - Add a `#[ignore = "#1173: headless drops DialogueOccurred"]` three-way
     case (harness vs headless vs live) documenting the known gap.
3. `docs(harness): move "parity golden" to mechanically-enforced`
   - Edit `docs/agent/harness.md` column + cite `parish-engine/tests/mode_parity.rs`.
4. Verify: `cargo nextest run -p parish-engine mode_parity`; run the live
   fixture for the transcript; write evidence.md + judge.md; `just agent-check`.

## PR 2 — #1173 chokepoint extraction (depends on PR 1)

5. `feat(core): apply_npc_turn shared per-turn chokepoint`
   - Add `apply_npc_turn` in `parish-core` alongside `apply_movement`
     (`game_session.rs` or a new `game_loop/npc_apply.rs`), doing all five
     steps inline; gate the newly-added steps behind
     `flags.is_enabled("turn-chokepoint")` (default-on).
   - Unit test over a `CapturingEmitter` / fresh `WorldState`.
6. `refactor(engine): harness calls apply_npc_turn`
   - Replace the duplicated body in `testing.rs`
     `consume_canned_npc_response` (and the addressed-turn handler) with a
     call. No copy-pasted per-turn body remains.
7. `refactor(engine): headless calls apply_npc_turn`
   - Replace `headless.rs` `apply_npc_response` body with a call. Headless now
     does name detection + `DialogueOccurred` (the bugfix).
8. `refactor(core): live loop calls apply_npc_turn`
   - `npc_turn.rs` calls the chokepoint for the inline steps; live loop now
     records `conversation_log` + witness memories (the bugfix). Keep its async
     inference/streaming wrapper.
9. `test(parity): un-ignore three-way parity case`
   - Remove the `#[ignore]`; all three paths now emit the same stream.
10. Verify: `cargo nextest run -p parish-engine mode_parity` green incl. the
    three-way case; `cargo test -p parish-core --test architecture_fitness`
    (rule 1/12) green; run the live fixture; evidence.md + judge.md;
    `just agent-check`; `just attach-proof`.

## Tests to add/update

- `parish-engine/tests/mode_parity.rs` (new, both PRs).
- `parish-core` unit test for `apply_npc_turn` (PR 2).
- Existing dialogue/regression tests (#1028, #1035, #1077/#1079) must stay
  green — they now exercise the shared seam.
- Watch `eval_baselines.rs` snapshots: live + headless now emit additional
  records; regenerate baselines with `just baselines` if drift is intended.

## Risks

- Headless/live newly recording `conversation_log` + witness memories may
  change snapshot baselines and any test asserting their absence — audit
  before regenerating.
- `detect_and_record_player_name` newly run in headless could set
  `world.player_name` in fixtures that previously left it unset; check
  `play_f20-harness-player-name.txt` and name-detection tests.
