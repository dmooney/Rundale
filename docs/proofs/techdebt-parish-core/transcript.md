Evidence type: gameplay transcript

## Summary of changes

Resolved 8 items from `parish/crates/parish-core/TODO.md` across dead-code deletion, duplication removal, doc fixes, and refactor categories (TD-015 through TD-022):

### Dead-code deletion
- **TD-015**: Deleted dead `interpolate_template` public API and its 4 tests from `game_mod.rs`
- **TD-016**: Updated stale test comment `kilteevan mod` → `rundale mod` in `game_mod.rs`

### Duplication removal
- **TD-017**: Extracted `weekday_name` helper in `ipc/handlers.rs`; deduplicated from `snapshot_from_world` and `build_clock_debug`
- **TD-021**: Extracted `run_autonomous_chain` helper from Phase 2 chain in `npc_turn.rs`, replacing a duplicated loop in `run_idle_banter`

### Dead parameter / suppression removal
- **TD-018**: Moved `peek_mod_id` into the `Err` arm of `load_setting_mod_sync`, eliminating the `let _ = setting_id;` suppression
- **TD-019**: Dropped dead `_transport` parameter from `snapshot_from_world`; updated all cross-crate call sites in `parish-tauri`, `parish-server`, and internal callers

### Docs
- **TD-020**: Fixed `prepare_npc_conversation` doc to describe it as an active single-target convenience wrapper used by the headless CLI, not an inactive stub
- **TD-022**: Added Rule 9 warning docs to `find_mods_root` and `LocalDiskModSource::new`, noting the cwd-walk is a dev fallback only and not safe in packaged builds

## Verification

### Cargo test output
```
test result: ok. 329 passed; 0 failed; 1 ignored; 0 measured (unit + integration suites)
```

### Cargo clippy
```
cargo clippy -p parish-core -- -D warnings
Finished - no warnings
```

### Full gate (fmt + clippy + test + witness-scan)
See judge.md for final verdict.
