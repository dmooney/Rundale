# Tech Debt Phase 1.3: parish-server Duplication Extractions

**Date:** 2026-05-07
**Items:** TD-001 through TD-005 (all Duplication category)

## Changes

### TD-001 — Cookie-value extraction (P2)

`cookie_value()` in `src/auth.rs` duplicated the `split(';')` → `strip_prefix` loop already present in `extract_cookie_value()` in `src/middleware.rs`. Replaced the body of `cookie_value()` to delegate to `extract_cookie_value()` for the inner `&str` parsing. The HeaderMap→&str step remains in `cookie_value()`.

### TD-002 — Google OAuth redirect URL (P2)

The `format!()` block constructing the Google consent-screen URL was identical in `login_google` and `login_google_tower`. Extracted into `build_google_oauth_url()` private helper.

### TD-003 — OAuth callback core (P2)

The 90-line body of `callback_google` and `callback_google_tower` that runs `exchange_code` → `fetch_user_info` → resolve/link session was identical. The only per-caller differences are CSRF state storage (cookie vs tower-session) and response construction (Set-Cookie headers vs tower-session insert). Extracted into `resolve_oauth_session()` private async helper parameterized over `current_session_id: Option<&str>`.

### TD-004 — Schema creation (P2)

The `CREATE TABLE sessions`, `CREATE TABLE oauth_accounts`, and `ALTER TABLE` migration were duplicated verbatim in `SessionRegistry::open()` and `open_sessions_db()`. Extracted into `apply_session_schema()` in `session_store_impl.rs`, called by both.

### TD-005 — AuthContext creation (P3)

The `resolve_account_id` + `record("account_id")` + `req.extensions_mut().insert(AuthContext{...})` block appeared three times in `cf_access_guard` (loopback bypass, JWT success, debug fallback). Extracted `make_auth_context()` helper that takes `identity_store`, `email`, and `flags`, returns an `AuthContext`. Each caller still handles `record()` and `extensions_mut().insert()` individually since the debug fallback path lacks the `record()` call.

## Files Changed

- `parish/crates/parish-server/src/auth.rs` — TD-001, TD-002, TD-003
- `parish/crates/parish-server/src/lib.rs` — TD-005
- `parish/crates/parish-server/src/session.rs` — TD-004 (import + call)
- `parish/crates/parish-server/src/session_store_impl.rs` — TD-004 (new helper + caller)
- `parish/crates/parish-server/TODO.md` — moved items to Done

## Verification

- `cargo test -p parish-server`: 173 unit tests pass, 15 integration suites (63 tests) pass
- `cargo clippy -p parish-server --all-targets -- -D warnings`: clean
- `cargo fmt -p parish-server`: clean
