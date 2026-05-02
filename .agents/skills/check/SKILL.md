---
name: check
description: Run the full cargo quality gate — fmt, clippy, and tests. Use before committing or when you want to verify code health.
disable-model-invocation: true
---

Run the full Rundale quality gate. All three must pass before any commit.

**Important:** The Cargo workspace lives in `parish/`. There is no `Cargo.toml` at the repo root. All cargo commands must run from inside `parish/`. Use the top-level `just` commands which handle the `cd` automatically, OR prefix cargo commands with `cd parish &&`.

Run `just check` from the repo root. This runs: `fmt-check`, `clippy`, `test`, `witness-scan`, and `check-doc-paths`.

If `just check` fails, diagnose by running the steps individually:
1. **Format**: `cd parish && cargo fmt --check`. Fix with `cd parish && cargo fmt`, then re-check.
2. **Lint**: `cd parish && cargo clippy -- -D warnings`. Fix warnings before proceeding.
3. **Tests**: `cd parish && cargo test`. All tests must pass.

Report a summary at the end:
- Which steps passed/failed
- If anything failed, show the relevant error output and suggest fixes
- Do NOT commit or push — just report status
