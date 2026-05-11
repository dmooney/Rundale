# Phase 1.2 — parish-server weak tests (TD-013 through TD-016)

## Summary

Added 10 new tests covering four untested route handlers and middleware in
`parish-server`:

| ID | Description | Tests | Location |
|----|-------------|-------|----------|
| TD-013 | `POST /api/session-init` — HMAC token minting | 2 | `tests/session_init.rs` |
| TD-014 | `GET /metrics` — Prometheus auth-failure counter | 2 | `src/lib.rs` test module |
| TD-015 | `GET /api/auth/status` — login-state display | 3 | `tests/auth_status.rs` |
| TD-016 | `ip_rate_limit_middleware` — per-IP rate limiting | 3 | `src/lib.rs` test module |

## Files changed

1. **`tests/session_init.rs`** (new) — integration tests for TD-013:
   - `session_init_returns_token_with_valid_auth` — verifies a valid HMAC token
     is minted and round-trips through `SessionToken::validate_full`.
   - `session_init_rejects_missing_auth_context` — confirms 500 when no
     `AuthContext` extension is present.

2. **`tests/auth_status.rs`** (new) — integration tests for TD-015:
   - `auth_status_no_auth_returns_logged_out` — no extension, no cookie.
   - `auth_status_with_extension_returns_logged_out` — `SessionId` extension
     present but no linked OAuth account.
   - `auth_status_with_cookie_returns_logged_out` — `parish_sid` cookie present
     but no linked OAuth account.

3. **`src/lib.rs`** (modified) — added test module entries for TD-014 and TD-016:
   - `get_metrics_returns_prometheus_format` — verifies `# HELP`, `# TYPE`, and
     counter line structure.
   - `get_metrics_reflects_auth_failures_counter` — sets counter to 42, reads
     output, restores original value.
   - `ip_rate_limit_allows_requests_within_quota` — 10 qps quota, single request
     returns 200.
   - `ip_rate_limit_rejects_requests_exceeding_quota` — 1 qps quota, two rapid
     requests: first 200, second 429.
   - `ip_rate_limit_loopback_is_exempt_in_debug` — loopback address bypasses
     the limiter even with aggressive quota.

4. **`TODO.md`** (modified) — moved TD-013 through TD-016 to Done section,
   added Phase 1.2 progress entry.

## Test results

```
cargo test -p parish-server
    173 unit tests passed
     63 integration tests passed (15 suites)
   236 total, 0 failed
cargo clippy -p parish-server --all-targets -- -D warnings
    clean
```

## New test count: 10
