# Phase 4.1 — WebSocket + OAuth integration tests (TD-011, TD-012)

## What changed

### TD-011 — WebSocket integration tests

**Observation**: `ws.rs` had only a placeholder compilation test. No integration tests covered `?token=` validation, single-connection enforcement (409), global cap (503), or message forwarding.

**Fix**:
- Extracted `validate_ws_upgrade()` from `ws_handler` — a pure function that validates the WS upgrade request (token, loopback bypass, auth context) without needing axum's `WebSocketUpgrade` extractor (which requires hyper's internal upgrade machinery).
- Refactored `ws_handler` to delegate to `validate_ws_upgrade()`.
- Added `tests/ws_integration.rs` with **8 tests**:
  - 7 tests for `validate_ws_upgrade` directly (missing token, invalid token, empty token, valid token, valid token with auth context, loopback bypass without token, loopback bypass with invalid token).
  - 1 real TCP integration test: starts a server on a random port, connects via `tokio-tungstenite`, emits an event on the event bus, and verifies it arrives over the WebSocket stream.
- Replaced the misleading `ws_module_compiles` placeholder with a `WsValidation` type sanity check.

### TD-012 — OAuth route integration tests

**Observation**: Six OAuth route handlers had no router-level integration tests. Only pure helpers (`exchange_code`, `fetch_user_info`, `urlenccode`) were tested via wiremock.

**Fix**:
- Added `tests/oauth_integration.rs` with **12 tests** covering all 6 handlers:
  - **Legacy handlers** (6 tests): `login_google` with/without OAuth, `callback_google` missing code, `callback_google` CSRF mismatch, `callback_google` provider error redirect, `logout`.
  - **Tower-session handlers** (6 tests): `login_google_tower` with/without OAuth, `callback_google_tower` missing code, CSRF mismatch, provider error redirect, `logout_tower`.

### Supporting changes
- Added `tokio-tungstenite = "0.29"` and `futures-util = "0.3"` as dev-dependencies in `Cargo.toml` for the WebSocket message-forwarding test.

## Test counts

- 8 new WS integration tests (`ws_integration.rs`)
- 12 new OAuth integration tests (`oauth_integration.rs`)
- Total: 168 unit + 17 integration suites (83 tests) — all pass
- Clippy: `-D warnings` clean
