Evidence type: gameplay transcript

## Summary of changes

Resolved 8 items from `parish/crates/parish-core/TODO.md` across config, code, test, and docs categories:

### Config
- **TD-001**: Removed unused `rand` dependency from `Cargo.toml`
- **TD-002**: Moved `regex` from `[dependencies]` to `[dev-dependencies]`

### Duplication
- **TD-003**: Eliminated `apply_arrival_reactions_inner` by replacing its single call site with `apply_arrival_reactions(..., &ReactionConfig::default())` and deleting the function

### Tests
- **TD-004**: Added 5 async tests for `TileCache::get()` covering SSRF guard, unknown source, cache miss/fetch/hit, and HTTP failure
- **TD-007**: Added `handle_system_command` dispatch tests with mock `SystemCommandHost`
- **TD-009**: Rewrote no-op test to actually verify non-panic behavior; removed dead code
- **TD-010**: Removed dead variable assignments from movement test

### Docs
- **TD-013**: Resolved `SessionStore` session ID doc contradiction between UUID v4 trait doc and `""` single-user convention
- **TD-014**: Fixed `lib.rs` module doc to describe parish-core as orchestration layer, not leaf-crate owner

### Deferred (Follow-up)
- TD-005 (DbSessionStore tests), TD-006 (save.rs tests), TD-008 (IdentityStore trait tests), TD-011 (handle_command complexity), TD-012 (debug snapshot complexity) — recorded as follow-up items requiring integration-level changes or carrying behavioral risk.

## Verification

### Cargo test output
```
test result: ok. 322 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out
(all test suites including integration tests)
```

### Cargo clippy
```
cargo clippy -p parish-core -- -D warnings
Finished - no warnings
```

### Full gate (fmt + clippy + test + agent-check + witness-scan)
See judge.md for final verdict.
