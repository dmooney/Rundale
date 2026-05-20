Verdict: sufficient
Technical debt: clear
Acceptance criteria: met

## Criterion verification

- **Detector is unit-tested.** The detector
  `parish_npc::quality::detect_emoji_monoculture(emojis: &[&str]) ->
  Option<QualityIssue>` and its `detect_emoji_monoculture_with_thresholds`
  variant landed in PR #990 and are exercised by tests in
  `parish/crates/parish-npc/src/quality.rs`. This change consumes that
  detector unchanged rather than duplicating it. `cargo test -p
  parish-npc` reports `466 passed`.

- **`NpcManager::record_reaction_emoji` uses a capacity-8 ring with
  debounced WARN emission.** Implementation in
  `parish/crates/parish-npc/src/manager.rs`. New constant
  `REACTION_EMOJI_BUFFER_CAPACITY = 8` plus a
  `reaction_monoculture_active` flag that debounces — the WARN fires
  once per crossing and the test
  `reaction_emoji_monoculture_clears_when_diversity_returns` verifies
  the flag resets when diversity recovers so a future crossing re-arms
  the sensor.

- **Every reaction-emit site records into the buffer.** Source grep:

  ```
  parish/crates/parish-cli/src/headless.rs:        app.npc_manager.record_reaction_emoji(&emoji);
  parish/crates/parish-cli/src/testing.rs:         self.app.npc_manager.record_reaction_emoji(&emoji);
  parish/crates/parish-server/src/routes.rs:       npc_manager.record_reaction_emoji(&emoji);
  parish/crates/parish-tauri/src/commands.rs:      npc_manager.record_reaction_emoji(&emoji);
  ```

  The sensor lives on the shared `NpcManager` rather than being
  open-coded per entry point — consistent with rule #12's
  cross-runtime-orchestration expectation.

- **`infer_player_message_reaction` now passes an explicit `1.0`
  temperature.** Verified in
  `parish/crates/parish-npc/src/reactions/emoji_reactions.rs`: the new
  `REACTION_INFERENCE_TEMPERATURE: f32 = 1.0` constant is threaded into
  `client.generate_json(..., Some(80), Some(REACTION_INFERENCE_TEMPERATURE))`,
  replacing the previous `None` (provider default ≈ 0). The 80-token
  output cap from PR #984 is preserved. A comment at the call site
  links the change to issue #995 and explains why widening sampling is
  safe (output schema is a closed palette).

- **Live CLI run shows the sensor in ACTIVE state after sustained
  same-keyword traffic.** Transcript line:

  ```
  [DEBUG REACTIONS]
    Buffer (8 / 8): 😠 😠 😠 😠 😠 😠 😠 😠
    Monoculture: ACTIVE — emoji diversity 1/8 = 12% (threshold 30%)
  ```

  Buffer at capacity, distinct/total = 1/8 = 12% (below the detector's
  default 30% diversity floor). The `/debug reactions` subcommand
  surfaces the same buffer that the structured
  `tracing::warn!(kind="reaction-emoji-monoculture", …)` event reports
  in production logs, so the proof transcript witnesses end-to-end
  wiring without depending on log-file access.

## Technical-debt review

No `#[allow]` attributes were added. No half-finished branches. The
sensor is observable in three independent ways: (a) the structured
`tracing::warn!` event from `record_reaction_emoji`, (b) the
`/debug reactions` CLI output, and (c) the
`NpcManager::reaction_emoji_buffer()` accessor used by tests. The
temperature constant is documented at its declaration. The
acceptance-criteria document was written first; the implementation
follows from it.
