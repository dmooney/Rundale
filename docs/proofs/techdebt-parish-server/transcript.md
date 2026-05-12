Evidence type: refactoring transcript

## Summary

Resolved 12 TODO items in `parish/crates/parish-server/TODO.md` (TD-021 through TD-032):

**Manifest Hygiene (3):**
- TD-021: Dropped unused `tower-http` `cors` feature from Cargo.toml
- TD-022: Removed unused `tracing-opentelemetry` and `opentelemetry` deps from Cargo.toml
- TD-023: Moved `tower` to `[dev-dependencies]` (only used in `#[cfg(test)]` modules)

**Stale TODO (1):**
- TD-024: Removed no-op `MemoryStore` cleanup task body in `lib.rs`; replaced with a comment explaining the 365-day expiry bound

**Duplication (2):**
- TD-025: Removed `google_account_for_session` from `SessionRegistry`; callers in `auth.rs` and `routes.rs` now use `global.identity_store.get_account`
- TD-026: Extracted `finalize_session_entry` helper shared by `create_session` and `restore_session`

**Naming (1):**
- TD-027: Renamed `urlenccode` → `urlencode` across definition, call sites, and tests

**Rule 9 Violation (1):**
- TD-028: Switched `ensure_saves_dir()` → `resolve_project_saves_dir(&data_dir)` in `lib.rs`

**Weak Tests (3):**
- TD-029: Added `parse_tile_path` unit tests in `tile_routes.rs` (valid path, missing suffix, too few/many segments, invalid coords, empty source, negative)
- TD-030: Added `react_to_message` tests (valid emoji → 200, invalid emoji → 400, injection snippet → 400)
- TD-031: Added `get_npcs_here_returns_json_array` smoke test

**Complexity/Hidden Bug (1):**
- TD-032: Fixed `restore_session` to select the most recently modified `.db` file via `spawn_blocking` + `sort_by_key(mtime)` instead of alphabetically first; prevents stale-branch restores when multiple save files exist

**Test results:**
```
cargo test -p parish-server
running 191 unit + 46 integration tests
test result: ok. 237 passed; 0 failed
```

**Clippy:**
```
cargo clippy -p parish-server --all-targets --all-features -- -D warnings
Finished (0 errors, 0 warnings)
```

**fmt:**
```
cargo fmt --all -- --check
(no output, clean)
```

**Workspace:**
```
cargo check --workspace
Finished (0 warnings)
```
