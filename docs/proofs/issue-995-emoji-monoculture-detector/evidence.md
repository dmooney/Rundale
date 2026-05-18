Evidence type: live gameplay transcript

# Evidence: issue-995-emoji-monoculture-detector

Captured by running the harness CLI against the new fixture and tee'ing
stdout to `transcript.txt`:

```
RUST_LOG=info cargo run --manifest-path parish/Cargo.toml -p parish \
  -- --script parish/testing/fixtures/play_issue-995-emoji-monoculture-detector.txt \
  2>&1 | tee docs/proofs/issue-995-emoji-monoculture-detector/transcript.txt
```

The CLI (`parish-cli`, package name `parish`) is one of the three
runtime paths listed in rule #10's live-proof tier, so this run
exercises the production reaction flow (rule-based reactions when no
LLM client is bound) and feeds every emitted emoji into the new
`NpcManager::record_reaction_emoji` sink.

## Background — relationship to #990

PR #990 already landed the pure detector
`parish_npc::quality::detect_emoji_monoculture(emojis: &[&str]) ->
Option<QualityIssue>` and its unit tests, but did not wire it to a
runtime buffer — the detector was reachable only from tests. Issue
#995 asked for a "session-scoped ring buffer (e.g. last 8 emoji on
`GameLoopContext` or `AppState`)" and a runtime call site that emits a
`WARN` once the detector fires.

This change adds exactly that runtime wiring and bumps the reaction
inference temperature so the small-model monoculture the sensor is
meant to catch is also actively diluted.

## Criterion → transcript lines

- **Reactions flow through the persist callback that feeds the buffer.**
  Each of the rent / tithe / landlord lines produces a visible reaction
  in `new_log_lines`. Sampled lines from `transcript.txt`:

  - `The rent collectors are out again this week.` → `Niamh Darcy 😠`, `Padraig Darcy 😠`
  - `The landlord wants tithe paid by Sunday.`     → `Niamh Darcy 😠`, `Padraig Darcy 😠`
  - `That agent of the landlord is back at the door.` → `Niamh Darcy 😠`
  - `The tithe collector knocked twice today.`    → `Niamh Darcy 😠`, `Padraig Darcy 😠`
  - `More rent talk — the landlord won't budge.`  → `Padraig Darcy 😠`
  - `Another eviction notice — the rent is impossible.` → `Padraig Darcy 😠`
  - `The rent collectors don't sleep.`            → `Niamh Darcy 😠`, `Padraig Darcy 😠`

- **`NpcManager::record_reaction_emoji` populates the rolling
  capacity-8 buffer and the detector runs against it.** Transcript
  contains:

  ```
  [DEBUG REACTIONS]
    Buffer (8 / 8): 😠 😠 😠 😠 😠 😠 😠 😠
    Monoculture: ACTIVE — emoji diversity 1/8 = 12% (threshold 30%)
  ```

  The buffer is at capacity (`8 / 8`), distinct emoji = 1 of 8 (12%),
  below the detector's 30% diversity threshold. `Monoculture: ACTIVE`
  is the in-game projection of the same state that drives the
  structured `tracing::warn!(site="reactions",
  kind="reaction-emoji-monoculture", detail=…)` event in
  `parish/crates/parish-npc/src/manager.rs::record_reaction_emoji`.

- **All four reaction-emit sites feed the sink.** Verified by source
  grep — each runtime calls
  `npc_manager.record_reaction_emoji(&emoji)` inside its own persist
  callback:

  - `parish/crates/parish-cli/src/headless.rs` (live LLM-or-fallback path)
  - `parish/crates/parish-cli/src/testing.rs` (script-harness path used for this transcript)
  - `parish/crates/parish-server/src/routes.rs`
  - `parish/crates/parish-tauri/src/commands.rs`

- **Reaction inference now uses an explicit non-default temperature.**
  `parish-npc/src/reactions/emoji_reactions.rs` defines
  `REACTION_INFERENCE_TEMPERATURE: f32 = 1.0` and passes it into
  `client.generate_json(...)` so small-model reaction sampling explores
  beyond the most-likely-safe choice. The 80-token output cap from
  #984 is preserved.

- **Detector + sensor unit tests pass.** `cargo test -p parish-npc`
  reports `466 passed (5 suites, 1.01s)` — including the pre-existing
  `quality::detect_emoji_monoculture*` tests from #990 plus the new
  manager regression tests in `parish/crates/parish-npc/src/manager.rs`:
  `reaction_emoji_buffer_caps_at_capacity`,
  `reaction_emoji_diverse_history_does_not_flag`,
  `reaction_emoji_monoculture_flips_active_state`,
  `reaction_emoji_monoculture_clears_when_diversity_returns`.
