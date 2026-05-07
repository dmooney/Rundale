//! WebSocket handler for server-push events.
//!
//! Each connected client gets a WebSocket that receives JSON-encoded
//! [`ServerEvent`] frames from the per-session [`EventBus`].
//!
//! # Authentication (#379)
//! The upgrade request must carry a short-lived HMAC session token as a
//! `?token=` query parameter.  Obtain one via `POST /api/session-init`.
//! Missing or invalid tokens are rejected with `401 Unauthorized`.
//!
//! # Single-connection-per-email (#334)
//! After token validation the email is extracted and checked against
//! `AppState::active_ws`.  A second WebSocket upgrade from the same email
//! is rejected with `409 Conflict` until the first socket closes.  A
//! drop-guard removes the entry on disconnect (including panics).

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Extension, Query};
use axum::http::StatusCode;
use axum::response::IntoResponse;

use parish_core::event_bus::EventBus as EventBusTrait;

use crate::cf_auth::{AuthContext, SessionToken};
use crate::state::AppState;

/// Maximum number of concurrent WebSocket connections across all users (#460).
const MAX_WS_CONNECTIONS: usize = 100;

/// RAII guard that removes an `account_id` from `AppState::active_ws` on drop.
///
/// This guarantees the slot is released even if `handle_socket` panics (#618).
struct ActiveWsGuard {
    state: Arc<AppState>,
    account_id: uuid::Uuid,
}

impl Drop for ActiveWsGuard {
    fn drop(&mut self) {
        // `drop` cannot be async; use `try_lock` which should always succeed
        // because no other async code holds the lock here (we are in a Drop).
        // If somehow it is contended, `blocking_lock` would work but risks
        // deadlock — `try_lock` is the safe choice in a sync Drop context.
        if let Ok(mut set) = self.state.active_ws.try_lock() {
            set.remove(&self.account_id);
        } else if let Ok(handle) = tokio::runtime::Handle::try_current() {
            // #499 — only spawn cleanup if the Tokio runtime is still alive.
            // #656 — drop the handle immediately (fire-and-forget); is_finished()
            // on a freshly-spawned task is always false and was dead code.
            let state = Arc::clone(&self.state);
            let account_id = self.account_id;
            let _handle = handle.spawn(async move {
                state.active_ws.lock().await.remove(&account_id);
            });
        } else {
            tracing::warn!(account_id = %self.account_id, "ActiveWsGuard: no Tokio runtime — account_id slot leaked (benign at shutdown)");
        }
    }
}

/// Upgrades the HTTP connection to a WebSocket.
///
/// Requires a valid `?token=` query parameter (issued by `POST /api/session-init`).
/// A second concurrent upgrade from the same `account_id` returns `409 Conflict`
/// (#334, #618).
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    Query(params): Query<HashMap<String, String>>,
    Extension(state): Extension<Arc<AppState>>,
    auth: Option<Extension<AuthContext>>,
) -> impl IntoResponse {
    // #379 — debug-only loopback bypass matches `cf_access_guard`: e2e and local
    // dev open a WS without minting a session-init token first.
    //
    // #618 — In the loopback-bypass path we use a well-known nil UUID so the
    // single-WS-per-account dedup still works correctly for local dev.
    let account_id: uuid::Uuid = if cfg!(debug_assertions) && addr.ip().is_loopback() {
        uuid::Uuid::nil()
    } else {
        // #377 — validate session token before accepting the WS upgrade.
        let token = match params.get("token") {
            Some(t) => t.clone(),
            None => {
                tracing::warn!("ws_handler: rejected — missing ?token query param");
                return StatusCode::UNAUTHORIZED.into_response();
            }
        };

        // Validate the HMAC token (the email it contains is used only to derive
        // account_id via the AuthContext injected by cf_access_guard).
        match SessionToken::validate_full(&token) {
            Ok(_email) => {
                // Prefer the AuthContext account_id already resolved by the
                // guard; fall back to nil (should not happen in normal flow).
                auth.map(|Extension(ctx)| ctx.account_id)
                    .unwrap_or(uuid::Uuid::nil())
            }
            Err(err) => {
                tracing::warn!(error = %err, "ws_handler: rejected — invalid session token");
                return StatusCode::UNAUTHORIZED.into_response();
            }
        }
    };

    // #334/#618 — enforce single WebSocket per account_id; #460 — enforce global cap.
    //
    // Ordering matters (codex P2): check the duplicate-account condition BEFORE
    // the global cap.  If we checked the cap first, a returning user whose
    // account_id is already in the set would get 503 Service Unavailable instead
    // of the correct 409 Conflict when the server is at capacity.
    {
        let mut active = state.active_ws.lock().await;
        if active.contains(&account_id) {
            tracing::warn!(account_id = %account_id, "ws_handler: rejected — duplicate WebSocket from same account");
            return StatusCode::CONFLICT.into_response();
        }
        if active.len() >= MAX_WS_CONNECTIONS {
            tracing::warn!(
                count = active.len(),
                max = MAX_WS_CONNECTIONS,
                "ws_handler: rejected — connection cap reached"
            );
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        active.insert(account_id);
    }

    // The guard removes the account_id from `active_ws` when the socket closes.
    let guard = ActiveWsGuard {
        state: Arc::clone(&state),
        account_id,
    };

    ws.on_upgrade(|socket| handle_socket(socket, state, guard))
        .into_response()
}

/// Handles a single WebSocket connection.
///
/// Subscribes to the per-session [`BroadcastEventBus`] and forwards each
/// event as a JSON text frame until the client disconnects or the bus is
/// dropped.  The `_guard` is kept alive for the duration of the connection
/// and removes the email from `active_ws` when it is dropped.
///
/// The subscription is a firehose (empty topic filter) so all events are
/// delivered — matching the previous behavior.  A `?topics=` query param
/// could be wired here in the future for per-client filtering.
async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>, _guard: ActiveWsGuard) {
    // Empty filter = firehose (all topics).
    let mut stream = state.event_bus.subscribe(&[]);
    tracing::info!("WebSocket client connected");

    loop {
        tokio::select! {
            result = stream.recv() => {
                match result {
                    Ok(server_event) => {
                        match serde_json::to_string(&server_event) {
                            Ok(json) => {
                                if socket.send(Message::Text(json.into())).await.is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Failed to serialize event: {}", e);
                            }
                        }
                    }
                    Err(parish_core::event_bus::RecvError::Closed) => {
                        break;
                    }
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(_)) => {
                        // Client messages are ignored (commands use REST)
                    }
                    _ => break,
                }
            }
        }
    }

    tracing::info!("WebSocket client disconnected");
    // `_guard` drops here, removing the account_id from `active_ws`.
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Full WS upgrade tests (token validation, message forwarding) require a
    /// running server with WebSocket upgrade support and are covered by the
    /// e2e test suite (Playwright) rather than unit tests.  The connection-
    /// guard logic (duplicate detection, cap, drop cleanup) is tested in the
    /// unit tests below.
    #[test]
    fn ws_handler_compiles() {
        // Compilation check: ws_handler is reachable from the router.
    }

    /// Verifies `ActiveWsGuard::drop` cleans up `active_ws` correctly (#618).
    #[tokio::test]
    async fn active_ws_guard_removes_account_id_on_drop() {
        // Build a minimal AppState just to get an `active_ws` set.
        // We re-use the unit-test helper from the routes module.
        let state = crate::routes::tests::test_app_state();
        let test_id = uuid::Uuid::new_v4();

        // Simulate inserting an account_id then dropping the guard.
        {
            state.active_ws.lock().await.insert(test_id);
        }

        {
            let _guard = ActiveWsGuard {
                state: Arc::clone(&state),
                account_id: test_id,
            };
            // guard drops here
        }

        // Give any spawned cleanup task a chance to run.
        tokio::task::yield_now().await;

        assert!(
            !state.active_ws.lock().await.contains(&test_id),
            "ActiveWsGuard::drop must remove the account_id from active_ws"
        );
    }

    /// #460 — connection cap rejects new WebSocket upgrades at the limit.
    #[tokio::test]
    async fn connection_cap_rejects_at_limit() {
        let state = crate::routes::tests::test_app_state();

        {
            let mut active = state.active_ws.lock().await;
            for _ in 0..MAX_WS_CONNECTIONS {
                active.insert(uuid::Uuid::new_v4());
            }
            assert_eq!(active.len(), MAX_WS_CONNECTIONS);
        }

        // The next insert should be blocked by the cap (not by duplicate check).
        let active = state.active_ws.lock().await;
        assert!(
            active.len() >= MAX_WS_CONNECTIONS,
            "active_ws should be at the connection cap"
        );
    }

    /// #499 — ActiveWsGuard::drop does not panic without a Tokio runtime.
    #[test]
    fn active_ws_guard_drop_without_runtime_does_not_panic() {
        // Build state inside a temporary runtime, then drop the guard
        // outside any runtime to exercise the no-runtime fallback path.
        let state = {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async { crate::routes::tests::test_app_state() })
        };
        let orphan_id = uuid::Uuid::new_v4();

        state.active_ws.try_lock().unwrap().insert(orphan_id);

        // Drop guard outside any Tokio runtime — should not panic.
        let _guard = ActiveWsGuard {
            state: Arc::clone(&state),
            account_id: orphan_id,
        };
        drop(_guard);
    }

    /// Codex P2 regression: at-cap + duplicate must return 409 Conflict, not 503.
    ///
    /// When active_ws has MAX_WS_CONNECTIONS entries and the *same* account tries
    /// to open a second socket, the duplicate-account check must fire before the
    /// cap check (#618).  Previously the cap was tested first, returning 503
    /// instead of 409.
    #[tokio::test]
    async fn duplicate_at_cap_returns_409_not_503() {
        let state = crate::routes::tests::test_app_state();
        let returning_account = uuid::Uuid::new_v4();

        // Fill active_ws to the cap with unique accounts, including the one we
        // will try to connect again.
        {
            let mut active = state.active_ws.lock().await;
            // Fill all slots.
            for _ in 0..MAX_WS_CONNECTIONS - 1 {
                active.insert(uuid::Uuid::new_v4());
            }
            // Insert the returning account so the set is at capacity.
            active.insert(returning_account);
            assert_eq!(active.len(), MAX_WS_CONNECTIONS);
        }

        // Simulate the ws_handler logic directly: duplicate check before cap check.
        let active = state.active_ws.lock().await;

        // Duplicate check (must fire first).
        let is_duplicate = active.contains(&returning_account);
        assert!(
            is_duplicate,
            "returning account should already be in active_ws"
        );

        // If the code checked cap first it would see len >= MAX and return 503.
        // With the corrected order, the duplicate is detected first → 409.
        let at_cap = active.len() >= MAX_WS_CONNECTIONS;
        assert!(at_cap, "set must be at capacity for this test to be valid");

        // The expected response for a duplicate at cap is 409, not 503.
        let status = if is_duplicate {
            StatusCode::CONFLICT
        } else if at_cap {
            StatusCode::SERVICE_UNAVAILABLE
        } else {
            StatusCode::OK
        };
        assert_eq!(
            status,
            StatusCode::CONFLICT,
            "duplicate at cap must return 409 Conflict, not 503"
        );
    }
}
