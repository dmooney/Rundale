//! NPC token streaming bridge for the web server.
//!
//! Reads tokens from an inference channel, applies separator holdback logic,
//! batches them, and emits `stream-token` events via the [`EventBus`].
//! This is a web-server-specific duplicate of `src-tauri/src/events.rs::stream_npc_response`,
//! adapted to emit through the [`EventBus`] instead of a Tauri `AppHandle`.

use std::time::{Duration, Instant};

use parish_core::ipc::StreamTokenPayload;
use parish_core::npc::{SEPARATOR_HOLDBACK, find_response_separator, floor_char_boundary};

use crate::state::EventBus;

/// How many milliseconds to batch streaming tokens before emitting.
const BATCH_MS: u64 = 16;

/// Reads tokens from `token_rx`, applies the NPC separator holdback logic,
/// batches them every [`BATCH_MS`] ms, and emits `stream-token` events.
///
/// Returns the full accumulated response text (including the hidden JSON
/// metadata section) so the caller can extract Irish word hints.
pub async fn stream_npc_response(
    bus: &EventBus,
    mut token_rx: tokio::sync::mpsc::UnboundedReceiver<String>,
) -> String {
    let mut accumulated = String::new();
    let mut displayed_len: usize = 0;
    let mut separator_found = false;
    let mut batch = String::new();
    let mut last_emit = Instant::now();

    while let Some(token) = token_rx.recv().await {
        accumulated.push_str(&token);

        if !separator_found {
            if let Some((dialogue_end, _meta_start)) = find_response_separator(&accumulated) {
                if dialogue_end > displayed_len {
                    batch.push_str(&accumulated[displayed_len..dialogue_end]);
                }
                displayed_len = dialogue_end;
                separator_found = true;
            } else {
                let raw_end = accumulated.len().saturating_sub(SEPARATOR_HOLDBACK);
                let safe_end = floor_char_boundary(&accumulated, raw_end);
                if safe_end > displayed_len {
                    batch.push_str(&accumulated[displayed_len..safe_end]);
                    displayed_len = safe_end;
                }
            }
        }

        if !batch.is_empty() && last_emit.elapsed() >= Duration::from_millis(BATCH_MS) {
            bus.emit(
                "stream-token",
                &StreamTokenPayload {
                    token: batch.clone(),
                },
            );
            batch.clear();
            last_emit = Instant::now();
        }
    }

    // Flush any remaining batch
    if !batch.is_empty() {
        bus.emit("stream-token", &StreamTokenPayload { token: batch });
    }

    // Flush any remaining displayed text if no separator was ever found
    if !separator_found && displayed_len < accumulated.len() {
        let remaining = accumulated[displayed_len..].to_string();
        if !remaining.is_empty() {
            bus.emit("stream-token", &StreamTokenPayload { token: remaining });
        }
    }

    accumulated
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn stream_simple_tokens() {
        let bus = EventBus::new(64);
        let mut rx = bus.subscribe();
        let (tx, token_rx) = mpsc::unbounded_channel();

        tx.send("Hello ".to_string()).unwrap();
        tx.send("world!".to_string()).unwrap();
        drop(tx);

        let full = stream_npc_response(&bus, token_rx).await;
        assert_eq!(full, "Hello world!");

        // At least one stream-token event should have been emitted
        let mut got_token = false;
        while let Ok(event) = rx.try_recv() {
            if event.event == "stream-token" {
                got_token = true;
            }
        }
        assert!(got_token);
    }
}
