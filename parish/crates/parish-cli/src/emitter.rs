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
//! use parish::emitter::StdoutEmitter;
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

impl StdoutEmitter {
    /// Returns the content string to print for a text-log event, or `None` if
    /// the event should be silently ignored (wrong event name, missing content,
    /// or empty content).
    fn format_event(name: &str, payload: &serde_json::Value) -> Option<String> {
        if name == "text-log" {
            payload
                .get("content")
                .and_then(|v| v.as_str())
                .filter(|c| !c.is_empty())
                .map(|s| s.to_string())
        } else {
            None
        }
    }
}

impl EventEmitter for StdoutEmitter {
    fn emit_event(&self, name: &str, payload: serde_json::Value) {
        if let Some(content) = Self::format_event(name, &payload) {
            println!("{}", content);
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
        let e = StdoutEmitter::new();
        let payload =
            serde_json::json!({"source": "system", "content": "Hello parish.", "id": "msg-1"});
        // Smoke test: must not panic
        e.emit_event("text-log", payload);
    }

    #[test]
    fn non_text_log_silent() {
        let e = StdoutEmitter::new();
        // Must not panic — these events should be silently ignored
        e.emit_event(
            "world-update",
            serde_json::json!({"location": "crossroads"}),
        );
        e.emit_event("stream-token", serde_json::json!({"token": "hello"}));
        e.emit_event("loading", serde_json::json!({"active": true}));
        e.emit_event("unknown-event", serde_json::Value::Null);
    }

    #[test]
    fn text_log_content_extracted() {
        let payload =
            serde_json::json!({"source": "system", "content": "Hello parish.", "id": "msg-1"});
        let result = StdoutEmitter::format_event("text-log", &payload);
        assert_eq!(result, Some("Hello parish.".to_string()));
    }

    #[test]
    fn text_log_empty_content_silent() {
        let payload = serde_json::json!({"content": ""});
        let result = StdoutEmitter::format_event("text-log", &payload);
        assert_eq!(result, None);
    }

    #[test]
    fn text_log_missing_content_silent() {
        let payload = serde_json::json!({"foo": "bar"});
        let result = StdoutEmitter::format_event("text-log", &payload);
        assert_eq!(result, None);
    }

    #[test]
    fn non_text_log_events_return_none() {
        let payload = serde_json::json!({"location": "crossroads"});
        let result = StdoutEmitter::format_event("world-update", &payload);
        assert_eq!(result, None);

        let result = StdoutEmitter::format_event("stream-token", &payload);
        assert_eq!(result, None);

        let result = StdoutEmitter::format_event("unknown-event", &serde_json::Value::Null);
        assert_eq!(result, None);
    }
}
