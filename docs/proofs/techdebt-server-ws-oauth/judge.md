Evidence type: code change + test run

## Evidence

1. `parish/crates/parish-server/tests/ws_integration.rs` — 8 WebSocket integration tests
2. `parish/crates/parish-server/tests/oauth_integration.rs` — 12 OAuth route integration tests
3. `parish/crates/parish-server/src/ws.rs` — `validate_ws_upgrade()` extracted, `ws_handler` refactored to delegate
4. `parish/crates/parish-server/Cargo.toml` — `tokio-tungstenite` + `futures-util` dev-deps added
5. `parish/crates/parish-server/TODO.md` — TD-011 and TD-012 moved to Done
6. `docs/proofs/techdebt-server-ws-oauth/` — this proof bundle

## Commands run

```sh
cargo test -p parish-server           # all pass
cargo clippy -p parish-server --all-targets -- -D warnings  # clean
```

## Files changed

| File | Change |
|------|--------|
| `parish/crates/parish-server/Cargo.toml` | Add `tokio-tungstenite`, `futures-util` dev-deps |
| `parish/crates/parish-server/src/ws.rs` | Extract `validate_ws_upgrade()`, add `WsValidation` type, replace placeholder test |
| `parish/crates/parish-server/tests/ws_integration.rs` | NEW — 8 tests |
| `parish/crates/parish-server/tests/oauth_integration.rs` | NEW — 12 tests |
| `parish/crates/parish-server/TODO.md` | Move TD-011, TD-012 to Done |
| `docs/proofs/techdebt-server-ws-oauth/transcript.md` | NEW |
| `docs/proofs/techdebt-server-ws-oauth/judge.md` | NEW |

## Verdict

Verdict: sufficient
Technical debt: clear

Both TD-011 and TD-012 are fully resolved. The WebSocket handler now has:
- Pure-function token validation tests (7 cases covering missing, invalid, empty, valid, AuthContext injection, loopback bypass).
- A real TCP message-forwarding test proving events flow from the event bus to the WebSocket.
- Refactored `validate_ws_upgrade()` that separates validation from the axum upgrade machinery, making future changes testable.

The OAuth routes now have router-level integration tests for all 6 handlers, covering both configured/unconfigured states, missing parameters, CSRF mismatches, provider errors, and logout.
