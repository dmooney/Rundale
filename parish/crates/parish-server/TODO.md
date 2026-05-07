# parish-server — Technical Debt

## Open

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-006 | Complexity | P2 | `src/lib.rs:362-908` | `run_server` is ~547 lines with 10+ distinct configuration phases. Each section is a separable constructor step. (Requires careful extraction; deferred to separate refactor.) |
| TD-007 | Complexity | P2 | `src/session.rs:956-1192` | `spawn_session_ticks` is ~230 lines with three nested async tasks. (Tightly coupled async code; deferred.) |
| TD-008 | Complexity | P2 | `src/session.rs:364-528` | `purge_expired_disk_sessions` is ~160 lines with nested closures and two-phase DB. (Deferred.) |
| TD-009 | Complexity | P3 | `src/routes.rs:979-1078` | `load_branch` is ~100 lines intermixing path canonicalization, containment checks, etc. (Deferred.) |
| TD-010 | Complexity | P2 | `src/middleware.rs:390-524` | `idempotency_middleware` is ~135 lines combining method gating, flag check, cache logic. (Deferred.) |

## In Progress

*(none)*

## Done

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-001 | Duplication | P2 | `src/auth.rs`, `src/middleware.rs` | Cookie-value extraction unified: `cookie_value()` now delegates to `extract_cookie_value()`. |
| TD-002 | Duplication | P2 | `src/auth.rs` | Google OAuth redirect URL extracted to `build_google_auth_url()` helper. |
| TD-003 | Duplication | P2 | `src/auth.rs` | OAuth callback session resolution extracted to `resolve_oauth_link()` helper. |
| TD-004 | Duplication | P2 | `src/session.rs`, `src/session_store_impl.rs` | Schema creation extracted to `initialize_sessions_schema()` in `session_store_impl.rs`. |
| TD-005 | Duplication | P3 | `src/lib.rs` | Auth context injection (`resolve_account_id` + `record()` + `insert(AuthContext)`) extracted to `inject_auth_context()` helper. |
| TD-011 | Weak Tests | P1 | `src/ws.rs` | Replaced placeholder test with docs clarifying coverage; guard/cap/duplicate tests already cover connection logic. |
| TD-012 | Weak Tests | P1 | `src/auth.rs` | Added 8 router-level integration tests for OAuth routes (login, login_tower, logout, logout_tower, callback error paths). |
| TD-013 | Weak Tests | P2 | `src/routes.rs` | Added `session_init_returns_token` test verifying 200 + HMAC token issuance. |
| TD-014 | Weak Tests | P2 | `src/lib.rs` | Added `metrics_returns_counter_in_plain_text` test for `/metrics`. |
| TD-015 | Weak Tests | P2 | `src/auth.rs` | Added `auth_status_no_oauth_no_session` test for `/api/auth/status`. |
| TD-016 | Weak Tests | P3 | `src/lib.rs` | Added `ip_rate_limit_middleware_blocks_at_capacity` functional test with router context. |
| TD-017 | Stale Docs | P3 | `src/routes.rs:12` | Removed stale `Semaphore` comment. |
| TD-018 | Stale Docs | P3 | `src/session_store_impl.rs:1-9` | Updated module doc to clarify `SqliteSessionRegistry` is removed and `session::SessionRegistry` is canonical. |
| TD-019 | Stale Docs | P3 | `src/lib.rs:104-107` | Converted `TODO:` to descriptive comment referencing issue #543. |
| TD-020 | Dead Code | P2 | `src/session_store_impl.rs:139-317` | Deleted `SqliteSessionRegistry` struct, trait impl, and associated tests. |

## Follow-up

Items requiring changes outside this crate or deferred due to risk of regression:

- **TD-006, TD-007, TD-008, TD-009, TD-010** — complexity refactors. Each is working, tested code. Extraction would require careful parameterization and is deferred to a dedicated refactor pass.
