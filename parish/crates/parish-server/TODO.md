# parish-server — Technical Debt

## Open

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-001 | Duplication | P2 | `src/auth.rs:642-658`, `src/middleware.rs:527-537` | Cookie-value extraction is duplicated. `cookie_value()` parses `HeaderMap`, `extract_cookie_value()` parses `&str`, but both implement identical `split(';')` → `strip_prefix` logic. Extract a single shared helper. |
| TD-002 | Duplication | P2 | `src/auth.rs:68-75`, `src/auth.rs:304-311` | Google OAuth redirect URL construction (client_id, redirect_uri, scope, state concatenation) is duplicated between `login_google` and `login_google_tower`. |
| TD-003 | Duplication | P2 | `src/auth.rs:90-238`, `src/auth.rs:337-485` | OAuth callback core logic (exchange_code → fetch_user_info → resolve/link session) is duplicated between `callback_google` and `callback_google_tower`. The only difference is CSRF state storage (cookie vs tower-sessions). Extract the shared body. |
| TD-004 | Duplication | P2 | `src/session.rs:180-195`, `src/session_store_impl.rs:327-348` | Schema creation (`CREATE TABLE sessions`, `CREATE TABLE oauth_accounts`) is duplicated identically in `SessionRegistry::open()` and `open_sessions_db()`. Both also apply the same `ALTER TABLE` migration. |
| TD-005 | Duplication | P3 | `src/lib.rs:269-279`, `src/lib.rs:297-308`, `src/lib.rs:346-353` | The `resolve_account_id` + `record("account_id")` + `req.extensions_mut().insert(AuthContext { ... })` block appears three times in `cf_access_guard` (loopback bypass, JWT success, debug fallback). |
| TD-006 | Complexity | P2 | `src/lib.rs:362-908` | `run_server` is ~547 lines with 10+ distinct configuration phases (env, world-path, LLM, mod, flags, saves, sessions, OAuth, WS key, tile cache, admission control, router build). Each section is a separable constructor step. |
| TD-007 | Complexity | P2 | `src/session.rs:956-1192` | `spawn_session_ticks` is ~230 lines with three deeply nested async tasks (world tick, inactivity tick, autosave tick), each containing lock acquisitions, error handling, and event emission inside closures. |
| TD-008 | Complexity | P2 | `src/session.rs:364-528` | `purge_expired_disk_sessions` is ~160 lines with nested closures, two-phase DB transaction management, dynamic placeholder construction, canonicalization, and security-guard loops all in one function. |
| TD-009 | Complexity | P3 | `src/routes.rs:979-1078` | `load_branch` is ~100 lines intermixing path canonicalization, containment checks, advisory lock acquisition, blocking DB operations, snapshot capture/restore, and event emission. |
| TD-010 | Complexity | P2 | `src/middleware.rs:390-524` | `idempotency_middleware` is ~135 lines: method gating, feature-flag check, header extraction, session scoping, LRU cache lookup+eviction, response buffering (capped at 1 MiB), and response reconstruction — all in one function. |
| TD-011 | Weak Tests | P1 | `src/ws.rs:197-200` | WebSocket handler has only a placeholder compilation test. The `ws.rs:198` comment admits "real WebSocket tests require a running server." No integration test covers WS upgrade, `?token=` validation, single-connection enforcement (`409 Conflict`), global cap (`503`), or message forwarding. |
| TD-012 | Weak Tests | P1 | `src/auth.rs:56-87, 90-238, 246-269, 286-329, 337-485, 492-519` | Six OAuth route handlers (`login_google`, `callback_google`, `logout`, `login_google_tower`, `callback_google_tower`, `logout_tower`) have no router-level integration tests. Only pure helpers (`exchange_code`, `fetch_user_info`, `urlenccode`) are tested via wiremock. |
| TD-013 | Weak Tests | P2 | `src/routes.rs:1378-1383` | `POST /api/session-init` has no test. This route mints HMAC session tokens used by every WebSocket client. |
| TD-014 | Weak Tests | P2 | `src/lib.rs:1091-1096` | `GET /metrics` has no test. The auth-failure counter is read without testing its increment/decrement behavior end-to-end. |
| TD-015 | Weak Tests | P2 | `src/auth.rs:528-564` | `GET /api/auth/status` handler has no test. Callers depend on this for frontend login-state display; the `Option<Extension<SessionId>>` extraction + `cookie_value` fallback path is untested. |
| TD-016 | Weak Tests | P3 | `src/lib.rs:1127-1159` | `ip_rate_limit_middleware` has no functional test in a router context. `extract_real_ip` is tested standalone, but the middleware's bridge between IP extraction and `governor::RateLimiter::check_key` is not exercised. |
| TD-017 | Stale Docs | P3 | `src/routes.rs:12` | Comment reads "// Semaphore is used by parish_core::game_loop::emit_npc_reactions (shared)" but `Semaphore` is not imported or used in this file. The comment is a leftover from a previous implementation. |
| TD-018 | Stale Docs | P3 | `src/session_store_impl.rs:1-9` | Module doc says `SqliteSessionRegistry` is defined here, but the production `SessionRegistry` in `session.rs` is a separate concrete type. The doc does not clarify why two registries exist or which one is canonical. |
| TD-019 | Stale Docs | P3 | `src/lib.rs:104-107` | TODO comment about replacing `'unsafe-inline'` in CSP `script-src` with `'sha256-...'` has existed since the CSP was first defined (issue #543). The referenced SvelteKit `kit.csp` config option has not been integrated. |
| TD-020 | Dead Code | P2 | `src/session_store_impl.rs:139-317` | `SqliteSessionRegistry` implements `SessionRegistryTrait` but is never used in production. `GlobalState.sessions` is typed as the concrete `session::SessionRegistry`, not `dyn SessionRegistryTrait`. Only its own unit tests construct this type. |

## In Progress

*(none)*

## Done

*(none)*
