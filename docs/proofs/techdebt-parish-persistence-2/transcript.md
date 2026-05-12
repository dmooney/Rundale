Evidence type: gameplay transcript

## Summary

Resolved 9 TODO items in `parish/crates/parish-persistence/TODO.md` (TD-015 through TD-023):

**Bug / Stale Logic (1):**
- TD-015: Dropped `NpcArrived`/`NpcDeparted` conversion in `journal_bridge.rs`; they now map to `None` (informational only). Updated tests.

**Rule 9 Violation (1):**
- TD-016: Deprecated `ensure_saves_dir()` with `#[deprecated]`; migrated callers in `parish-server` and `parish-cli` to `resolve_project_saves_dir`.

**Dead API (3):**
- TD-017: Downgraded `journal_bridge::to_journal_event` and `drain_events` visibility from `pub` to `pub(crate)`.
- TD-018: Downgraded `picker::PickerChoice` and `read_picker_choice` visibility from `pub` to `pub(crate)`.
- TD-019: Removed `lock::SaveFileLock::covers_path` and its test (`test_covers_path`).

**Test Hygiene (1):**
- TD-020: Fixed `test_ensure_saves_dir_creates_directory` to acquire `env_test_lock` before mutating `current_dir`.

**Inconsistent Logging (1):**
- TD-021: Switched `ensure_saves_dir_at` `eprintln`/`println` to `tracing::warn!`/`tracing::info!`.

**Weak Tests (1):**
- TD-022: Added `test_replay_dialogue_occurred_is_noop` in `journal.rs`.

**Stale Doc Comment (1):**
- TD-023: Fixed `ensure_saves_dir_at` doc comment to describe idempotent migration instead of one-time migration.

## Verification

### Cargo test output
```
cargo test -p parish-persistence
test result: ok. 115 passed; 0 failed; 0 ignored
```

### Cargo clippy
```
cargo clippy -p parish-persistence --all-targets -- -D warnings
Finished - no warnings
```

### fmt
```
cargo fmt --all -- --check
(no output, clean)
```

### Dependent crates
`parish-core`, `parish-cli`, `parish-server` checked clean after TD-016 migration.
