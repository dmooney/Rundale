//! [`StdoutEmitter`] — [`EventEmitter`] implementation for headless CLI mode.
//!
//! Prints `text-log` content to stdout (matching existing headless behaviour).
//! All other event types (world-update, stream-token, loading, travel-start,
//! ui-control, etc.) are silently ignored — the headless CLI has no graphical
//! UI to receive them.
//!
//! `stream-token` events are intentionally no-op'd: the headless runner already
//! buffers and prints the final dialogue once per NPC turn.  Token streaming
//! would produce broken line output in a terminal.
//!
//! # Usage
//!
//! ```rust,ignore
//! use parish_cli::emitter::StdoutEmitter;
//! use parish_core::ipc::EventEmitter;
//! use std::sync::Arc;
//!
//! let emitter: Arc<dyn EventEmitter> = Arc::new(StdoutEmitter::new());
//! emitter.emit_event("text-log", serde_json::json!({"content": "Hello"}));
//! // prints: Hello
//! ```

use parish_core::ipc::EventEmitter;

/// [`EventEmitter`] that prints `text-log` events to stdout.
///
/// All other events are silently discarded.
pub struct StdoutEmitter;

impl StdoutEmitter {
    /// Creates a new stdout emitter.
    pub fn new() -> Self {
        Self
    }
}

impl Default for StdoutEmitter {
    fn default() -> Self {
        Self::new()
    }
}

impl EventEmitter for StdoutEmitter {
    fn emit_event(&self, name: &str, payload: serde_json::Value) {
        if name == "text-log" {
            // Extract the `content` field and print it.
            if let Some(content) = payload
                .get("content")
                .and_then(|v| v.as_str())
                .filter(|c| !c.is_empty())
            {
                println!("{}", content);
            }
        }
        // All other event types (world-update, stream-token, loading, npc-reaction,
        // travel-start, ui-control variants) are ignored — no graphical UI to update.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_log_printed() {
        // This just tests compilation and no-panic; stdout capture is not
        // straightforward in Rust unit tests without extra crates.
        let e = StdoutEmitter::new();
        e.emit_event(
            "text-log",
            serde_json::json!({"source": "system", "content": "Hello parish.", "id": "msg-1"}),
        );
    }

    #[test]
    fn non_text_log_silent() {
        let e = StdoutEmitter::new();
        // Must not panic
        e.emit_event(
            "world-update",
            serde_json::json!({"location": "crossroads"}),
        );
        e.emit_event("stream-token", serde_json::json!({"token": "hello"}));
        e.emit_event("loading", serde_json::json!({"active": true}));
        e.emit_event("unknown-event", serde_json::Value::Null);
    }
}
