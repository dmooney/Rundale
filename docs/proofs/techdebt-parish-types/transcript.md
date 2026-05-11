Evidence type: gameplay transcript

## Summary

Resolved 10 additional TODO.md items in `parish/crates/parish-types` (TD-008 through TD-017):

- **TD-008**: Removed dead code `GameSpeed::factor_with_config` from `src/time.rs` (inlined into `factor`)
- **TD-009**: Removed dead code `ConversationLog::last_speaker_at` from `src/conversation.rs`
- **TD-010**: Removed dead code `GossipNetwork::recent` from `src/gossip.rs` and its test
- **TD-011**: Fixed broken intra-doc link in `src/events.rs` module doc
- **TD-012**: Updated `README.md` module list to include `lib.rs` re-exports
- **TD-013**: Added `Serialize`/`Deserialize` derives to `Festival`, `TimeOfDay`, `Weather`, `GameSpeed`, `SpeedConfig`
- **TD-014**: Replaced `GossipNetwork::create` sort+drain eviction with `VecDeque`+`pop_front`
- **TD-015**: Added Display output tests for `Festival`, `Season`, `TimeOfDay`, `GameSpeed`
- **TD-016**: Added EventBus overflow and lag tests in `src/events.rs`
- **TD-017**: Added `GameClock::set_speed` while frozen test in `src/time.rs`

## Files changed

- `parish/crates/parish-types/src/time.rs` — removed dead code, added derives, added tests
- `parish/crates/parish-types/src/conversation.rs` — removed dead code
- `parish/crates/parish-types/src/gossip.rs` — removed dead code, refactored eviction to VecDeque
- `parish/crates/parish-types/src/events.rs` — fixed doc link, added tests
- `parish/crates/parish-types/src/ids.rs` — added Serialize/Deserialize derive to Weather
- `parish/crates/parish-types/README.md` — updated module list
- `parish/crates/parish-types/TODO.md` — moved items to Done

## Test results

```
cargo test -p parish-types
running 122 tests
test result: ok. 122 passed; 0 failed
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

## Workspace check

```
cargo check -p parish-core   # PASS
cargo check -p parish        # PASS
```

## Agent check

```
just agent-check: passed
```
