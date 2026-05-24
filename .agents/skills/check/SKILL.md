---
name: check
description: Run the Rundale quality gates — `just check` (pre-commit) and `just verify` (pre-push, adds the game-harness walkthrough) — and diagnose failures, including the known CI false positives. Use before committing, before pushing, or when debugging a CI failure.
disable-model-invocation: true
paths:
  - .github/**
  - parish/scripts/**
  - justfile
---

Run the Rundale quality gates. There are two levels — run the one that matches where you are.

**Important:** the Cargo workspace lives in `parish/`. There is no `Cargo.toml` at the repo root. Use the
top-level `just` commands (they `cd parish` for you) OR prefix cargo commands with `cd parish &&`.

## Level 1 — `just check` (before every commit)

Run `just check` from the repo root. This runs: `agent-check`, `fmt-check`, `clippy`, `test`,
`witness-scan`, and `check-doc-paths`.

If it fails, diagnose by running the steps individually:
1. **Proof gate**: `just agent-check`. Add or fix the proof bundle under `.proofs/<task-id>/` when it
   reports missing evidence. The bundle is posted to the PR via `just attach-proof <task-id>`; it is not
   committed.
2. **Format**: `cd parish && cargo fmt --check`. Fix with `cd parish && cargo fmt`, then re-check.
3. **Lint**: `cd parish && cargo clippy -- -D warnings`. Fix warnings before proceeding.
4. **Tests**: `cd parish && cargo test`. All tests must pass.

Report which steps passed/failed with relevant error output and suggested fixes. Do NOT commit or push —
just report status.

## Level 2 — `just verify` (before every push)

Run `just verify` from the repo root. This runs everything in `just check` **plus** the game-harness
walkthrough script — it's the full pre-push gate.

If it fails, diagnose the shared steps as above, then:
5. **Game harness**: `cd parish && cargo run -p parish -- --script testing/fixtures/test_walkthrough.txt`
   and inspect the JSON output for correctness.

Only if ALL steps pass, confirm it is safe to push. If any step fails, stop and report — do NOT push; fix
the issue first.

## CI false positives & gotchas

Known quirks where CI reports a failure that isn't a real defect.

### agent-check: debt-marker false positives

`parish/scripts/agent-check.sh` runs `scan_for_debt_markers()` over every changed file. It greps for:
```
todo!\(
unimplemented!\(
unreachable!\(
panic!("Not implemented
panic!("Todo
// unchanged
// existing
// ...
/* ... */
pass # TODO
return nil // placeholder
```

When a PR touches **only documentation or design files** and the prose mentions Rust macro names (e.g. a
table of allowed change categories), these regexes match the prose. CI reports: `placeholder-like debt
markers found in changed files`.

**How to spot:** check the file. If the match is inside a Markdown table cell, inline code backticks, or
prose → false positive. If in `.rs` source → real debt.

**Fix:** rephrase prose so the patterns don't match:

| Before (matches) | After (safe) |
|---|---|
| `` `todo!()`, `unimplemented!()` `` | `` `todo!` / `unimplemented!` calls `` |
| `panic!("Not implemented ...")` | `panic!("unimplemented ...")` |

**Known skip-list gap:** `agent-check.sh` skips `parish/scripts/agent-check.sh`, `parish/justfile`, and
`docs/agent/witness.md`. It does NOT skip `docs/design/*`. If false positives from design docs become
frequent, add `docs/design/*` to the skip list.
