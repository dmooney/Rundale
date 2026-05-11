---
name: rundale-ci-pitfalls
description: Common false positives and quirks in Rundale CI checks. Load alongside land, check, or verify when debugging CI failures.
---

## agent-check: debt marker false positives

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

When a PR touches **only documentation or design files** and the prose mentions Rust macro names (e.g. a table of allowed change categories), these regex patterns match the prose. CI reports: `placeholder-like debt markers found in changed files`.

### How to spot

Check the file. If the match is inside a Markdown table cell, inline code backticks, or prose → false positive. If in `.rs` source → real debt.

### Fix

Rephrase prose so patterns don't match:

| Before (matches) | After (safe) |
|---|---|
| `` `todo!()`, `unimplemented!()` `` | `` `todo!` / `unimplemented!` calls `` |
| `panic!("Not implemented ...")` | `panic!("unimplemented ...")` |

### Known skip-list gap

`agent-check.sh` skips `parish/scripts/agent-check.sh`, `parish/justfile`, and `docs/agent/witness.md`. It does NOT skip `docs/design/*`. If false positives from design docs become frequent, add `docs/design/*` to the skip list.
