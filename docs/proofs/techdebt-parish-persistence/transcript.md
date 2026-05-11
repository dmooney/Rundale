Evidence type: gameplay transcript

## Summary

All 14 TODO.md items for `parish-persistence` resolved via refactoring and test additions:

**Dead Code (TD-001, TD-002):** Removed unused `anyhow` and `thiserror` dependencies from Cargo.toml.

**Duplication (TD-003, TD-004, TD-005):** Extracted `branch_info_from_row` helper in Database, added `run_blocking` generic method to eliminate 130 lines of async boilerplate across 9 `AsyncDatabase` methods, merged duplicate journal sequence tests.

**Weak Tests (TD-006–TD-011):** Added 9 new tests covering: NpcMoved replay (happy path + unknown-NPC skip), RelationshipChanged replay (strength adjust + unknown-NPC + missing-relationship skips), MemoryAdded replay (entry creation + unknown-NPC skip), corrupt JSON recovery in `load_latest_snapshot`, concurrent `append_event` sequence correctness (4 tasks x 25 events), and custom speed-factor fallback in `restore`.

**Complexity (TD-012, TD-013):** Split `GameSnapshot::restore()` into `restore_clock`, `restore_world_locations`, `restore_npcs` private helpers. Extracted `apply_player_moved` and `apply_memory_added` helpers from `replay_journal`.

**Stale Docs (TD-014):** Added `tracing::warn!` to the corrupt-file skip path in `discover_saves`.

## Cargo test output

```
test result: ok. 114 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.09s
```

## Cargo clippy output

```
Finished dev profile [unoptimized + debuginfo] target(s) in 0.09s
```

## Cargo fmt --check

```
(no output - clean)
```
