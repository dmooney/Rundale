//! WebSocket handler for server-push events.
//!
//! Each connected client gets a WebSocket that receives JSON-encoded
//! [`ServerEvent`] frames from the [`EventBus`].

use std::sync::Arc;

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;

use crate::state::AppState;

/// Upgrades the HTTP connection to a WebSocket.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

/// Handles a single WebSocket connection.
///
/// Subscribes to the [`EventBus`] and forwards each event as a JSON text
/// frame until the client disconnects or the bus is dropped.
async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
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
}

#[cfg(test)]
mod tests {
    #[test]
    fn ws_module_compiles() {
        // Placeholder — real WebSocket tests require a running server
    }
}
