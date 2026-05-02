---
name: verify
description: Full pre-push verification checklist. Runs fmt, clippy, tests, and game harness to ensure everything is ready to push.
disable-model-invocation: true
---

Run the complete Rundale pre-push verification checklist.

**Important:** The Cargo workspace lives in `parish/`. There is no `Cargo.toml` at the repo root. Use `just` commands from the repo root, which handle `cd parish &&` internally.

## Steps

Run `just verify` from the repo root. This runs: fmt-check, clippy, tests, witness-scan, doc-paths, and the game harness walkthrough script.

If `just verify` fails, diagnose by running steps individually:
1. **Format check**: `cd parish && cargo fmt --check`. Fix with `cd parish && cargo fmt`, then report what changed.
2. **Lint**: `cd parish && cargo clippy -- -D warnings`. Fix any warnings before proceeding.
3. **Tests**: `cd parish && cargo test`. All tests must pass.
4. **Game harness**: `cd parish && cargo run -p parish -- --script testing/fixtures/test_walkthrough.txt` and inspect JSON output for correctness.

5. **Summary**: Report pass/fail for each step. Only if ALL steps pass, confirm it is safe to push.

If any step fails, stop and report the failure. Do NOT push. Fix the issue first.
