Evidence type: gameplay transcript

## Summary

Resolved 15 of 20 TODO items in `parish/crates/parish-server/TODO.md`:

**Duplication (5):**
- TD-001: Unified cookie-value extraction (`cookie_value` delegates to `extract_cookie_value`)
- TD-002: Extracted `build_google_auth_url()` helper for OAuth redirect URL
- TD-003: Extracted `resolve_oauth_link()` helper shared by both OAuth callbacks
- TD-004: Extracted `initialize_sessions_schema()` shared by `SessionRegistry::open()` and `open_sessions_db()`
- TD-005: Extracted `inject_auth_context()` helper for the three auth paths in `cf_access_guard`

**Dead Code (1):**
- TD-020: Deleted `SqliteSessionRegistry` struct, trait impl, and tests (unused in production)

**Stale Docs (3):**
- TD-017: Removed stale `Semaphore` comment in routes.rs
- TD-018: Updated module doc in session_store_impl.rs
- TD-019: Converted TODO comment to descriptive reference in lib.rs

**Weak Tests (6):**
- TD-011: Replaced WS placeholder test with documentation clarifying coverage
- TD-012: Added 8 router-level integration tests for OAuth routes
- TD-013: Added `session_init_returns_token` test
- TD-014: Added `metrics_returns_counter_in_plain_text` test
- TD-015: Added `auth_status_no_oauth_no_session` test
- TD-016: Added `ip_rate_limit_middleware_blocks_at_capacity` test

**Test results:**
```
running 180 tests
test result: ok. 180 passed; 0 failed
```

**Clippy:**
```
cargo clippy -p parish-server -- -D warnings ... Finished
```

**fmt:**
```
cargo fmt --check ... (no output, clean)
```
