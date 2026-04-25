# Witness-style Completion Gates for Parish

This document describes the Parish witness workflow — a lightweight, repo-native guard against AI partial completions.

## Why we need this

Large AI-assisted refactors can report "done" while silently leaving placeholders, unwired code paths, or partial edits. The `witness-scan` recipe is a fast, deterministic Tier-0 guardrail that catches common markers before they land on `main`.

## Minimum gate for every AI-generated change

1. Draft an Oath file in `.agent/oaths/<task-id>.md` describing scope and postconditions.
2. Implement the changes.
3. Run `just check` (includes `witness-scan` automatically).
4. Run targeted tests for touched crates.
5. Append the outcome to `.agent/witness-log.md` with date, branch, and commit SHA.
6. Only then mark the task complete.

## Oath template

```md
# Oath: <task-id>

## Scope
- crates/parish-core/src/...
- crates/parish-server/src/...

## Postconditions (Tier 0)
- [ ] `just check` (includes `just witness-scan`)
- [ ] `cargo test -p parish-core <targeted-test>`
- [ ] `rg -n "<new_symbol>" <expected_callsite_file>` returns >= 1 match

## Postconditions (Tier 1)
- [ ] Reviewer confirms new function is wired from caller -> callee.
- [ ] Reviewer confirms no fallback/placeholder branch handles primary flow.

## Honest failure text
"Partial completion: <x>/<y> checks passed. Missing: <...>."
```

## What witness-scan checks

`just witness-scan` inspects every file under `crates/`, `apps/`, `docs/`, `testing/`, and `mods/` that is modified relative to the merge-base with `origin/main`. It fails loudly if any of these patterns appear:

| Pattern | Catches |
|---|---|
| `//​ unchanged`, `//​ existing` | AI stubs that left original code in place |
| `//​ … rest of the function`, `//​ …` | Ellipsis omissions |
| `/*​ … ​*/` | Block-comment omissions |
| `todo!(…)`, `unimplemented!(…)`, `unreachable!(…)` | Rust macro placeholders (with or without a message argument) |
| `panic!("Not implemented…")`, `panic!("todo…")` | Rust panic stubs (case-insensitive prefix match) |
| `pass # TODO` | Python placeholders |
| `return nil //​ placeholder` | Go placeholders |

## Witness log

`.agent/witness-log.md` is an append-only verification log. The file uses `merge=union` in `.gitattributes` so concurrent branches auto-merge without conflicts.

Log format:

```
- Date (UTC): YYYY-MM-DD HH:MM
- Branch: `<branch-name>`
- Commit: `<sha>`
- Oath: `.agent/oaths/<task-id>.md`
- Result: PASS | FAIL
- Checks run:
  - `just check`
- Notes: <missing items or confidence notes>
```

## Loud failure format

Do not end an AI task with "done" unless every Tier 0 check passes.

- PASS: `Witness: 6/6 PASS` + command list.
- FAIL: `Partial completion: 4/6 checks passed` + unmet checks listed explicitly.
