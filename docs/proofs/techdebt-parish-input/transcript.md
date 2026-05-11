Evidence type: gameplay transcript

## Summary

Resolved all 8 technical debt items from `parish/crates/parish-input/TODO.md`:

### Changes

1. **TD-001** (`Cargo.toml`): Removed unused `anyhow` dependency.
2. **TD-002** (`src/intent_local.rs`): Extracted `try_move_prefix` helper to eliminate duplicated byte-offset computation and `PlayerIntent` construction between `move_phrases` and `move_verbs` loops.
3. **TD-003** (`src/lib.rs`): Added 5 unit tests for `validate_flag_name` (empty, valid, max length 64, too long, invalid chars).
4. **TD-004** (`src/lib.rs`): Added 7 unit tests for `/flag` command family (bare, list, enable, disable, enable-bare-shows-list, invalid subcommand, invalid name, `/flags` alias).
5. **TD-005** (`src/lib.rs`): Added 2 unit tests for music session aliases (`/session`, `/tune`, `/music`, `/fiddle`, `/seisiun` and case-insensitivity).
6. **TD-006** (`src/lib.rs`): Added 3 unit tests for `/weather` (bare, set, case-insensitive).
7. **TD-007** (`src/parser.rs`): Extracted `parse_zero_arg_command` from `parse_system_command` to reduce the match body below 100 lines.
8. **TD-008** (`src/intent_local.rs`, `src/lib.rs`): Added `"move "` to `move_verbs` so bare `move pub` matches locally; added `test_local_parse_move_bare` test.

### Files modified

- `parish/crates/parish-input/Cargo.toml` — removed anyhow dep
- `parish/crates/parish-input/src/intent_local.rs` — `try_move_prefix` helper, added "move " to verbs
- `parish/crates/parish-input/src/parser.rs` — extracted `parse_zero_arg_command`
- `parish/crates/parish-input/src/lib.rs` — +18 test functions, validate_flag_name import
- `parish/crates/parish-input/TODO.md` — moved all items to Done

### Test output

```
running 137 tests
test result: ok. 137 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 6 tests (llm_fallback_integration)
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests parish_input
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### Clippy output

No warnings, no errors.

### Discovery scan

A post-fix discovery scan of the crate found no new credible technical debt. All dependencies are actively used, no dead code, no stale comments, and test coverage now includes previously untested areas.
