Evidence type: gameplay transcript

## Summary

Resolved 7 TODO.md items in `parish/crates/parish-types`:

- **TD-001**: Removed dead code (`check_festival_data`, `HasFestivalDate`) from `src/time.rs`
- **TD-002**: Added `Serialize` to `AnachronismEntry` in `src/lib.rs` (recorded follow-up for `parish-core` copy removal)
- **TD-003**: Added 13 `ParishError` tests covering all variants, Display messages, `#[from]` conversions
- **TD-004**: Added 17 `GameClock` tests for pause/resume, inference pause/resume, speed control, accessors
- **TD-005**: Extracted `handle_json_unicode_escape` helper from `extract_dialogue_from_partial_json` (both functions under 100 lines)
- **TD-006**: Removed unused `tokio-test` dev-dependency from `Cargo.toml`
- **TD-007**: Updated stale `distort` doc comment to match 3-rule implementation

## Files changed

- `parish/crates/parish-types/src/time.rs` — removed dead code, added tests
- `parish/crates/parish-types/src/error.rs` — added tests
- `parish/crates/parish-types/src/ids.rs` — extracted helper function
- `parish/crates/parish-types/src/gossip.rs` — fixed doc comment
- `parish/crates/parish-types/src/lib.rs` — added Serialize derive to AnachronismEntry
- `parish/crates/parish-types/Cargo.toml` — removed tokio-test dev-dependency
- `parish/crates/parish-types/TODO.md` — moved items to Done

## Test results

```
cargo test -p parish-types
running 116 tests
test result: ok. 116 passed; 0 failed
```

## Clippy results

```
cargo clippy -p parish-types -- -D warnings
no warnings (exit 0)
```

## Format check

```
cargo fmt --check -p parish-types
no diffs (exit 0)
```

## Witness scan

```
just witness-scan: passed
```

## Agent check

```
just agent-check: passed
```
