# Acceptance Criteria: issue-995-emoji-monoculture-detector

## Task

Issue #995 reports reaction emoji come back mostly the same (`🤔` /
`😊`) when the 1.5B model is the reaction backend — narrow distribution
at temp=0 on a small model. Lowest-effort path is option (6) in the
issue: wire the existing diversity detector (landed in PR #990 as
`parish_npc::quality::detect_emoji_monoculture`) to a session-scoped
rolling buffer that the runtime feeds on every emitted reaction. Then
add option (2) — an explicit temperature on the reaction inference
call — so the small model is forced to explore the full palette.

After this change:

- `NpcManager` carries a fixed-capacity ring buffer of the most recent
  reaction emoji and a `record_reaction_emoji` method that pushes onto
  the buffer and runs `parish_npc::quality::detect_emoji_monoculture`.
  When the detector returns `Some(QualityIssue)`, the method emits a
  structured `tracing::warn!(site="reactions",
  kind="reaction-emoji-monoculture", detail=…)` event. The detector
  returns `None` when the buffer is too small or sufficiently diverse,
  and the method debounces so the same crossing isn't re-logged.
- Every reaction-emit path (CLI headless, CLI script harness, web
  server, Tauri) records each emitted reaction emoji into the buffer —
  so the sensor sees every reaction regardless of runtime.
- `infer_player_message_reaction` passes an explicit `temperature =
  1.0` to `generate_json` (was `None`) so small-model reaction
  sampling widens.
- A live CLI proof transcript drives enough reactions of the same
  keyword group to populate the buffer past the minimum sample
  threshold; the resulting `/debug reactions` output reads
  `Monoculture: ACTIVE`, proving the sensor is wired end-to-end.

## Criteria

- `NpcManager::record_reaction_emoji` pushes onto a capacity-8 ring
  buffer and only emits the WARN once per crossing (debounced).
  observable via: unit test that records 8× same emoji and asserts the
  active flag flips, plus a recovery test that flushes with distinct
  emoji and asserts the flag clears.
- All four runtime reaction-emit sites call `record_reaction_emoji`.
  observable via: grep of `record_reaction_emoji` in
  `parish-cli/src/headless.rs`, `parish-cli/src/testing.rs`,
  `parish-server/src/routes.rs`, and `parish-tauri/src/commands.rs`.
- `infer_player_message_reaction` passes `Some(1.0)` for temperature.
  observable via: source diff plus the `REACTION_INFERENCE_TEMPERATURE`
  constant declaration.
- Live CLI run drives the buffer into monoculture; the new
  `/debug reactions` command reports `Monoculture: ACTIVE — emoji
  diversity 1/8 = 12% (threshold 30%)`. observable via: transcript
  capture under `docs/proofs/.../transcript.txt`.
- `cargo test -p parish-npc` passes (the existing detector tests from
  PR #990 plus the new `manager::tests::reaction_emoji_*` cases).

## Verification script

Run:

```
RUST_LOG=info cargo run --manifest-path parish/Cargo.toml -p parish \
  -- --script parish/testing/fixtures/play_issue-995-emoji-monoculture-detector.txt \
  2>&1 | tee docs/proofs/issue-995-emoji-monoculture-detector/transcript.txt
```

Expected signals in output:

- Multiple `Padraig Darcy 😠` / `Niamh Darcy 😠` reaction lines from
  rule-based fallback for repeated rent/landlord lines (proves
  reactions flow through the persist callback that feeds the buffer).
- A `[DEBUG REACTIONS]` block with a fully-saturated buffer and
  `Monoculture: ACTIVE — emoji diversity 1/8 = 12% (threshold 30%)`
  (proves the detector ran against the live buffer).
