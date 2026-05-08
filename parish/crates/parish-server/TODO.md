# parish-server — Technical Debt

## Open

*(none)*

## In Progress

*(none)*

## Done

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-001 | Duplication | P2 | `src/auth.rs`, `src/middleware.rs` | `cookie_value()` now delegates to `extract_cookie_value()` from middleware, eliminating the duplicated `split(';')` → `strip_prefix` loop. |
| TD-002 | Duplication | P2 | `src/auth.rs` | Extracted `build_google_oauth_url()` helper used by both `login_google` and `login_google_tower`. |
| TD-003 | Duplication | P2 | `src/auth.rs` | Extracted `resolve_oauth_session()` helper used by both `callback_google` and `callback_google_tower`. The only per-caller code is CSRF state storage (cookie vs tower-sessions) and response construction. |
| TD-004 | Duplication | P2 | `src/session.rs`, `src/session_store_impl.rs` | Extracted `apply_session_schema()` helper in session_store_impl.rs, called by both `SessionRegistry::open()` and `open_sessions_db()`. |
| TD-005 | Duplication | P3 | `src/lib.rs` | Extracted `make_auth_context()` helper used by all three `cf_access_guard` auth paths (loopback bypass, JWT success, debug fallback). |
| TD-006 | Complexity | P2 | `src/lib.rs` | `run_server` broken into separable constructor steps: `handle_dotenv`, `resolve_world_path`, `run_llm_bootstrap`, `resolve_splash_and_theme`, `resolve_engine_and_ui_config`, `open_session_components`, `check_ws_signing_key_warning`, `init_tile_cache`, `resolve_admission_control`, `spawn_session_cleanup_background_task`, `build_ip_rate_limiter_state`, `should_use_tower_sessions`. Router build remains inline. |
| TD-007 | Complexity | P2 | `src/session.rs` | `spawn_session_ticks` decomposed into `spawn_world_tick`, `spawn_inactivity_tick`, and `spawn_autosave_tick`. |
| TD-008 | Complexity | P2 | `src/session.rs` | `purge_expired_disk_sessions` extracted into `collect_and_delete_expired_ids` (nested method) and `cleanup_expired_session_dirs` (free function). |
| TD-009 | Complexity | P3 | `src/routes.rs` | `load_branch` extracted into `validate_and_acquire_lock`, `load_branch_snapshot`, and `restore_snapshot_and_emit`. |
| TD-010 | Complexity | P2 | `src/middleware.rs` | `idempotency_middleware` cache branches extracted into `try_replay_from_cache` and `cache_successful_response`. |
| TD-017 | Stale Docs | P3 | `src/routes.rs` | Removed stale "Semaphore is used by..." comment — `Semaphore` is not imported or used in that file. |
| TD-018 | Stale Docs | P3 | `src/session_store_impl.rs` | Updated module doc to clarify why a separate `session::SessionRegistry` exists alongside `SqliteIdentityStore`, and noted that `SqliteSessionRegistry` was a previous trait-based attempt that has been removed. |
| TD-019 | Stale Docs | P3 | `src/auth.rs`, `tests/security_headers.rs` | Replaced TODO comment about replacing `'unsafe-inline'` in CSP `script-src` with a deferred-design note referencing #543. |
| TD-020 | Dead Code | P2 | `src/session.rs`, `src/session_store_impl.rs` | Removed `SqliteSessionRegistry` struct and all its method implementations, unit tests, and dead imports. The canonical `session::SessionRegistry` is the only production registry. |
| TD-013 | Weak Tests | P2 | `tests/session_init.rs` | Added 2 integration tests for `POST /api/session-init`: happy path (valid HMAC token round-trips through `SessionToken::validate_full`) and missing AuthContext (500). |
| TD-014 | Weak Tests | P2 | `src/lib.rs` | Added unit tests for `GET /metrics` — verifies Prometheus format structure and counter value reflection. |
| TD-015 | Weak Tests | P2 | `tests/auth_status.rs` | Added 3 integration tests for `GET /api/auth/status`: no auth (logged_out), Extension&lt;SessionId&gt; path (logged_out), cookie fallback path (logged_out). |
| TD-016 | Weak Tests | P3 | `src/lib.rs` | Added functional tests for `ip_rate_limit_middleware` — 3 tests: within quota (200), exceeding quota (429), loopback bypass in debug builds (200). |
| TD-011 | Weak Tests | P1 | `src/ws.rs`, `tests/ws_integration.rs` | Extracted `validate_ws_upgrade()` from `ws_handler` — 7 unit-style tests for token validation (`?token=` missing/invalid/valid, loopback bypass, AuthContext injection), plus 1 real TCP message-forwarding test via `tokio-tungstenite`. |
| TD-012 | Weak Tests | P1 | `tests/oauth_integration.rs` | 12 router-level integration tests for all 6 OAuth route handlers (legacy + tower-sessions variants): 404 when unconfigured, 303 redirect when configured, missing code → 400, CSRF mismatch → 400, provider error → redirect, logout → 303 with new cookie. |

### Progress Log

- **2026-05-07**: Phase 1.1 — resolved TD-017 through TD-020 (stale docs + dead code). Removed `SqliteSessionRegistry` (~180 lines dead code), cleaned up CMS docs, comments, and dead imports. All 168 unit tests, 15 integration suites pass; clippy clean with `-D warnings`.
- **2026-05-07**: Phase 1.2 — resolved TD-013 through TD-016 (weak tests). Added 10 new tests: 2 for `POST /api/session-init` (session_init.rs), 2 for `GET /metrics` (lib.rs), 3 for `GET /api/auth/status` (auth_status.rs), and 3 for `ip_rate_limit_middleware` (lib.rs). Total: 173 unit tests + 15 integration suites (63 tests); clippy clean with `-D warnings`.
- **2026-05-07**: Phase 1.3 — resolved TD-001 through TD-005 (duplication extractions). Extracted `build_google_oauth_url()`, `resolve_oauth_session()`, `apply_session_schema()`, and `make_auth_context()` helpers; `cookie_value()` now delegates to `extract_cookie_value()`. All 173 unit tests + 15 integration suites (63 tests) pass; clippy clean with `-D warnings`.
- **2026-05-07**: Phase 3.1 — resolved TD-006 through TD-010 (parish-server complexity). Extracted 10 helpers from `run_server` (env, world-path, LLM, splash/theme, engine/UI, session components, WS key, tile cache, admission control, cleanup task, rate limiter, tower-sessions check). Decomposed `spawn_session_ticks` into 3 sub-functions, `purge_expired_disk_sessions` into 2 helpers, `load_branch` into 3 helpers, and `idempotency_middleware` cache logic into 2 helpers. All 168 unit tests + 15 integration suites (63 tests) pass; clippy clean with `-D warnings`.
- **2026-05-07**: Phase 4.1 — resolved TD-011 and TD-012 (WebSocket + OAuth weak tests). Extracted `validate_ws_upgrade()` from `ws_handler` for direct token-validation testing. Added 8 WS integration tests (7 `validate_ws_upgrade` + 1 real TCP message forwarding via tokio-tungstenite) and 12 OAuth router-level tests covering all 6 handlers (legacy + tower-sessions variants). All 168 unit tests + 17 integration suites (83 tests) pass; clippy clean with `-D warnings`.
