Evidence type: gameplay transcript

## Summary of changes

Follow-up to #921. The 2026-05-07 sweep listed several `parish-core` debt items as Done in the table but the underlying code didn't fully match. This PR closes that gap.

### Genuinely resolved this round

- **TD-002**: `regex` was still in `[dependencies]` despite the prior Done entry. Only `tests/architecture_fitness.rs` uses it. Removed the runtime entry from `parish/crates/parish-core/Cargo.toml`; the dev-dependency stays.
- **TD-009**: the no-op test had only been renamed (`apply_arrival_reactions_empty_location` → `apply_arrival_reactions_does_not_panic`), still asserting nothing. Deleted it — `apply_arrival_reactions_returns_empty_when_no_location_data` in `src/game_session.rs` already covers the empty-location fast-path with a controlled `WorldState::new()` fixture and a real assertion that the returned vec is empty.
- **TD-010**: the weak `apply_movement_already_here` test (`assert!(!effects.messages.is_empty())`) is redundant given `apply_movement_already_here_explicit`, which asserts the player location is unchanged and the log was appended. Deleted the weak duplicate.

### TODO.md reconciliation

Pruned the stale **Open** table:
- TD-001/003/013/014 — were genuinely fixed in #921 but never removed from Open.
- TD-004/007 — also genuinely fixed in #921 (tile_cache and system_command tests verified present) but still listed Open.
- TD-005/006/008/011/012 — were duplicated in both Open and Follow-up; consolidated into Follow-up.

Added a 2026-05-08 reconciliation note explaining the gap between the Done table and reality so the discrepancy is auditable.

## Verification

### cargo check
```
cargo check -p parish-core
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 36.89s
```

### cargo test (all targets)
```
cargo test -p parish-core
    test result: ok. 316 passed; 0 failed; 1 ignored (lib unit)
    test result: ok. 14 passed; 0 failed (integration)
    test result: ok. 6 passed; 0 failed (wiring_parity)
    test result: ok. 3 passed; 0 failed (architecture_fitness — regex usage in tests/ unaffected)
```

### cargo clippy
```
cargo clippy -p parish-core --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 16.63s
    (no warnings)
```

## Behavioural impact

None. Three cleanup deltas:
1. Cargo.toml: removed a runtime dep that was never imported from `src/`.
2. Two dead unit tests deleted; the surviving tests provide stronger coverage of the same paths.
3. TODO.md updated to reflect actual code state.

No production code was modified; no gameplay flow was touched; no IPC handlers, persistence paths, or runtime orchestration changed. See judge.md for verdict.
