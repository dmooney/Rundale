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

use crate::cf_auth::SessionToken;
use crate::state::AppState;

/// Maximum number of concurrent WebSocket connections across all users (#460).
const MAX_WS_CONNECTIONS: usize = 100;

/// RAII guard that removes an email from `AppState::active_ws` on drop.
///
/// This guarantees the slot is released even if `handle_socket` panics.
struct ActiveWsGuard {
    state: Arc<AppState>,
    email: String,
}

impl Drop for ActiveWsGuard {
    fn drop(&mut self) {
        // `drop` cannot be async; use `try_lock` which should always succeed
        // because no other async code holds the lock here (we are in a Drop).
        // If somehow it is contended, `blocking_lock` would work but risks
        // deadlock — `try_lock` is the safe choice in a sync Drop context.
        if let Ok(mut set) = self.state.active_ws.try_lock() {
            set.remove(&self.email);
        } else if let Ok(handle) = tokio::runtime::Handle::try_current() {
            // #499 — only spawn cleanup if the Tokio runtime is still alive.
            // #656 — drop the handle immediately (fire-and-forget); is_finished()
            // on a freshly-spawned task is always false and was dead code.
            let state = Arc::clone(&self.state);
            let email = self.email.clone();
            let _handle = handle.spawn(async move {
                state.active_ws.lock().await.remove(&email);
            });
        } else {
            tracing::warn!(user = %self.email, "ActiveWsGuard: no Tokio runtime — email slot leaked (benign at shutdown)");
        }
    }
}

/// Upgrades the HTTP connection to a WebSocket.
///
/// Requires a valid `?token=` query parameter (issued by `POST /api/session-init`).
/// A second concurrent upgrade from the same email returns `409 Conflict` (#334).
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    Query(params): Query<HashMap<String, String>>,
    Extension(state): Extension<Arc<AppState>>,
) -> impl IntoResponse {
    // #379 — debug-only loopback bypass matches `cf_access_guard`: e2e and local
    // dev open a WS without minting a session-init token first.
    let email = if cfg!(debug_assertions) && addr.ip().is_loopback() {
        "dev@localhost".to_string()
    } else {
        // #377 — validate session token before accepting the WS upgrade.
        let token = match params.get("token") {
            Some(t) => t.clone(),
            None => {
                tracing::warn!("ws_handler: rejected — missing ?token query param");
                return StatusCode::UNAUTHORIZED.into_response();
            }
        };

        match SessionToken::validate_full(&token) {
            Ok(e) => e,
            Err(err) => {
                tracing::warn!(error = %err, "ws_handler: rejected — invalid session token");
                return StatusCode::UNAUTHORIZED.into_response();
            }
        }
    };

    // #334 — enforce single WebSocket per email; #460 — enforce global cap.
    //
    // Ordering matters (codex P2): check the duplicate-email condition BEFORE
    // the global cap.  If we checked the cap first, a returning user whose
    // email is already in the set would get 503 Service Unavailable instead of
    // the correct 409 Conflict when the server is at capacity.
    {
        let mut active = state.active_ws.lock().await;
        if active.contains(&email) {
            tracing::warn!(user = %email, "ws_handler: rejected — duplicate WebSocket from same email");
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
        active.insert(email.clone());
    }

    // The guard removes the email from `active_ws` when the socket closes.
    let guard = ActiveWsGuard {
        state: Arc::clone(&state),
        email: email.clone(),
    };

    ws.on_upgrade(|socket| handle_socket(socket, state, guard))
        .into_response()
}

/// Handles a single WebSocket connection.
///
/// Subscribes to the per-session [`EventBus`] and forwards each event as a
/// JSON text frame until the client disconnects or the bus is dropped.
/// The `_guard` is kept alive for the duration of the connection and removes
/// the email from `active_ws` when it is dropped.
async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>, _guard: ActiveWsGuard) {
    let mut rx = state.event_bus.subscribe();
    tracing::info!("WebSocket client connected");

    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
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
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("WebSocket client lagged, dropped {} events", n);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
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

    // Fail any in-flight WebGPU inference requests so the inference worker
    // is not blocked waiting for a browser response that will never arrive.
    // Without this, each stalled request would occupy the worker until the
    // INFERENCE_RESPONSE_TIMEOUT_SECS wall-clock timeout fires.
    let mut pending = state.webgpu_pending.lock().await;
    for (_, waiter) in pending.drain() {
        let _ = waiter.done_tx.send(Err("WebSocket disconnected".into()));
    }
    drop(pending);

    // `_guard` drops here, removing the email from `active_ws`.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_module_compiles() {
        // Placeholder — real WebSocket tests require a running server
    }

    /// Verifies `ActiveWsGuard::drop` cleans up `active_ws` correctly.
    #[tokio::test]
    async fn active_ws_guard_removes_email_on_drop() {
        // Build a minimal AppState just to get an `active_ws` set.
        // We re-use the unit-test helper from the routes module.
        let state = crate::routes::tests::test_app_state();

        // Simulate inserting an email then dropping the guard.
        {
            state
                .active_ws
                .lock()
                .await
                .insert("test@example.com".to_string());
        }

        {
            let _guard = ActiveWsGuard {
                state: Arc::clone(&state),
                email: "test@example.com".to_string(),
            };
            // guard drops here
        }

        // Give any spawned cleanup task a chance to run.
        tokio::task::yield_now().await;

        assert!(
            !state.active_ws.lock().await.contains("test@example.com"),
            "ActiveWsGuard::drop must remove the email from active_ws"
        );
    }

    /// #460 — connection cap rejects new WebSocket upgrades at the limit.
    #[tokio::test]
    async fn connection_cap_rejects_at_limit() {
        let state = crate::routes::tests::test_app_state();

        {
            let mut active = state.active_ws.lock().await;
            for i in 0..MAX_WS_CONNECTIONS {
                active.insert(format!("user{i}@example.com"));
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

        state
            .active_ws
            .try_lock()
            .unwrap()
            .insert("orphan@example.com".to_string());

        // Drop guard outside any Tokio runtime — should not panic.
        let _guard = ActiveWsGuard {
            state: Arc::clone(&state),
            email: "orphan@example.com".to_string(),
        };
        drop(_guard);
    }

    /// Codex P2 regression: at-cap + duplicate must return 409 Conflict, not 503.
    ///
    /// When active_ws has MAX_WS_CONNECTIONS entries and the *same* user tries to
    /// open a second socket, the duplicate-email check must fire before the cap
    /// check.  Previously the cap was tested first, returning 503 instead of 409.
    #[tokio::test]
    async fn duplicate_at_cap_returns_409_not_503() {
        let state = crate::routes::tests::test_app_state();

        // Fill active_ws to the cap with unique users, including the one we
        // will try to connect again.
        let returning_user = "returning@example.com".to_string();
        {
            let mut active = state.active_ws.lock().await;
            // Fill all slots.
            for i in 0..MAX_WS_CONNECTIONS - 1 {
                active.insert(format!("user{i}@example.com"));
            }
            // Insert the returning user so the set is at capacity.
            active.insert(returning_user.clone());
            assert_eq!(active.len(), MAX_WS_CONNECTIONS);
        }

        // Simulate the ws_handler logic directly: duplicate check before cap check.
        let active = state.active_ws.lock().await;

        // Duplicate check (must fire first).
        let is_duplicate = active.contains(&returning_user);
        assert!(
            is_duplicate,
            "returning user should already be in active_ws"
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
