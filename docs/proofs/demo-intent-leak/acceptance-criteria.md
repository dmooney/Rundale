# Acceptance Criteria: demo-intent-leak

## Task

The demo auto-player currently leaks the LLM's internal intent-reasoning (e.g., `ask about the places nearby that are worth visiting`) as the player's literal chat input instead of natural first-person speech. The fix must tighten the demo prompt to forbid command-form output, add a runtime guard to detect and reject command-form intent leaks at the parse layer, and verify that natural speech passes through unchanged.

## Criteria

- **No command-form chat input in demo turns** — `just demo 2 5` produces zero chat utterances in command-form (bare verb-first patterns like `ask about ...`, `tell Name ...`, `whisper to ...`, `look at ...`, `go to ...`). Observable via: grep output for chat input lines matching those patterns.
- **Guard rejects intent-leak examples** — Unit test verifies that the guard correctly rejects `ask about the places nearby that are worth visiting` and similar command-form strings. Observable via: cargo test output showing the guard test passing.
- **Bare system commands still pass** — System commands without objects (`look`, `go`, `wait`) pass through the guard unchanged. Observable via: unit test confirming these valid commands are not rejected.
- **Natural speech passes unchanged** — First-person utterances and normal dialogue pass through the guard without modification. Observable via: unit test verifying natural speech strings are accepted; demo transcript showing natural dialogue in turns 1-5.

## Verification script

Run: `cargo run --manifest-path parish/Cargo.toml -p parish-cli -- --script parish/testing/fixtures/play_demo-intent-leak.txt`

Expected signals in output:
- Chat lines in `log` field containing first-person speech (e.g., "Good morning", "I've come from", "What news")
- Zero lines matching `chat \[player\] input=(ask about |tell |whisper |look at |go to )` in the demo log
- `cargo test -p parish-tauri -- demo_tests::is_command_form_intent_leak` showing test pass
- Guard test output confirming rejection of `ask about the places nearby`
