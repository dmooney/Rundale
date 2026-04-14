# Witness-style Completion Gates for Parish

This document translates TEMM1E's "Oath / Witness / Ledger" model into a lightweight, repo-native workflow for Parish.

## Why we need this

Large AI-assisted refactors can report "done" while silently leaving placeholders, unwired code paths, or partial edits. In Parish, this risk is highest for umbrella tasks that touch multiple crates (`parish-core`, `parish-server`, UI, etc.).

## Parish adaptation of the Five Laws

### 1) Pre-commitment (`Oath`)

Before running a coding agent, create a task contract in `.agent/oaths/<task-id>.md` with:

- **Scope**: exact files/modules expected to change.
- **Deterministic checks (Tier 0)**: commands that must pass.
- **Semantic checks (Tier 1/Tier 2)**: human or LLM review prompts focused on behavior.
- **Failure message template**: exact wording for partial completion.

Use this template:

```md
# Oath: <task-id>

## Scope
- crates/parish-core/src/...
- crates/parish-server/src/...

## Postconditions (Tier 0)
- [ ] `just witness-scan`
- [ ] `cargo test -p parish-core <targeted-test>`
- [ ] `rg -n "<new_symbol>" <expected_callsite_file>` returns >= 1 match

## Postconditions (Tier 1)
- [ ] Reviewer confirms new function is wired from caller -> callee.
- [ ] Reviewer confirms no fallback/placeholder branch handles primary flow.

## Honest failure text
"Partial completion: <x>/<y> checks passed. Missing: <...>."
```

### 2) Independent verdict (`Witness`)

The verifier must not rely on the agent summary. For now:

- Run Tier 0 checks from shell (`just witness-scan` + targeted tests).
- Review only changed files + test outputs.
- If any check fails, report partial completion explicitly.

### 3) Immutable-ish history (`Ledger`)

Parish does not currently use a hash-chained DB ledger. Until then, use a commit-linked log:

- Store Oaths in `.agent/oaths/`.
- Append verification outcomes in `.agent/witness-log.md` with commit SHA.
- Never edit old entries; add new entries only.

(If needed later, we can implement a SQLite hash chain crate in `crates/parish-core` or a dedicated crate.)

### 4) Loud failure

Do not end an AI task with "done" unless every Tier 0 check passes.

Use this format:

- `PASS`: `Witness: 6/6 PASS` + command list.
- `FAIL`: `Partial completion: 4/6 checks passed` + unmet checks.

### 5) Narrative-only fail

Verification should not auto-revert or auto-delete files. Keep verification read-only and report-only.

## Minimum gate for every AI-generated refactor

1. Draft Oath file.
2. Implement changes.
3. Run `just witness-scan`.
4. Run targeted tests for touched crates.
5. Record outcome in `.agent/witness-log.md`.
6. Only then mark task complete.

## Future hardening roadmap

- Add a Rust CLI (`parish-witness`) for structured Oath JSON + deterministic predicates.
- Add CI job that fails PRs if Oath is missing for large multi-file AI changes.
- Add append-only signed attestations (Sigstore/Git note or SQLite hash-chain).
