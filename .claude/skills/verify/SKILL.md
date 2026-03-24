---
name: verify
description: Full pre-push verification checklist. Runs fmt, clippy, tests, and game harness to ensure everything is ready to push.
disable-model-invocation: true
---

Run the complete Parish pre-push verification checklist.

## Steps

1. **Format check**: Run `cargo fmt --check`. If it fails, run `cargo fmt` to fix, then report what changed.
2. **Lint**: Run `cargo clippy -- -D warnings`. Fix any warnings before proceeding.
3. **Tests**: Run `cargo test`. All tests must pass.
4. **Game harness**: Run `cargo run -- --script tests/fixtures/test_walkthrough.txt` and inspect the JSON output for correctness.
5. **Summary**: Report pass/fail for each step. Only if ALL steps pass, confirm it is safe to push.

If any step fails, stop and report the failure. Do NOT push. Fix the issue first.
