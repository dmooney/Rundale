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
        } else {
            // Fallback: spawn a task to do the cleanup asynchronously.
            let state = Arc::clone(&self.state);
            let email = self.email.clone();
            tokio::spawn(async move {
                state.active_ws.lock().await.remove(&email);
            });
        }
    }
}

/// Upgrades the HTTP connection to a WebSocket.
///
/// Requires a valid `?token=` query parameter (issued by `POST /api/session-init`).
/// A second concurrent upgrade from the same email returns `409 Conflict` (#334).
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<HashMap<String, String>>,
    Extension(state): Extension<Arc<AppState>>,
) -> impl IntoResponse {
    // #379 — validate session token before accepting the WS upgrade.
    let token = match params.get("token") {
        Some(t) => t.clone(),
        None => {
            tracing::warn!("ws_handler: rejected — missing ?token query param");
            return StatusCode::UNAUTHORIZED.into_response();
        }
    };

    let email = match SessionToken::validate_full(&token) {
        Ok(e) => e,
        Err(err) => {
            tracing::warn!(error = %err, "ws_handler: rejected — invalid session token");
            return StatusCode::UNAUTHORIZED.into_response();
        }
    };

    // #334 — enforce single WebSocket per email.
    {
        let mut active = state.active_ws.lock().await;
        if !active.insert(email.clone()) {
            tracing::warn!(user = %email, "ws_handler: rejected — duplicate WebSocket from same email");
            return StatusCode::CONFLICT.into_response();
        }
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
}
