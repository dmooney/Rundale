---
name: check
description: Run the full cargo quality gate — fmt, clippy, and tests. Use before committing or when you want to verify code health.
disable-model-invocation: true
---

Run the full Rundale quality gate. All three must pass before any commit.

1. **Format check**: Run `cargo fmt --check`. If it fails, run `cargo fmt` to fix, then re-check.
2. **Lint**: Run `cargo clippy -- -D warnings`. Fix any warnings before proceeding.
3. **Tests**: Run `cargo test`. All tests must pass.

Report a summary at the end:
- Which steps passed/failed
- If anything failed, show the relevant error output and suggest fixes
- Do NOT commit or push — just report status
