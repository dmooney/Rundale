//! Abstract event-emission trait shared by all Parish backends.
//!
//! The three backends emit events differently:
//!
//! - `parish-tauri` uses `tauri::AppHandle::emit(name, payload)`.
//! - `parish-server` uses `EventBus::emit_named(topic, name, &payload)` which
//!   serialises via `serde_json::to_value` and broadcasts to WebSocket clients.
//! - `parish-cli` headless mode uses `StdoutEmitter` which logs `text-log`
//!   payloads to stdout and no-ops on all other event types.
//!
//! This trait is **object-safe** (no generic methods) so `Arc<dyn EventEmitter>`
//! can be passed into shared game-loop helpers (`run_npc_turn`,
//! `handle_npc_conversation`, etc.) without duplicating them.  Serialisation is
//! the caller's responsibility: callers convert their typed payload to
//! `serde_json::Value` before calling `emit_event`.
//!
//! # Backend implementations
//!
//! Each backend supplies a thin newtype:
//!
//! - **`parish-server`**: `BroadcastEmitter(Arc<BroadcastEventBus>)` → maps
//!   each `(name, payload)` to the correct [`Topic`] and calls `emit_named`.
//! - **`parish-tauri`**: `TauriEmitter(tauri::AppHandle)` → calls
//!   `app.emit(name, payload)`.
//! - **`parish-cli`**: `StdoutEmitter` → prints `text-log` content to stdout;
//!   no-ops on `world-update`, `stream-token`, `loading`, etc.

/// Backend-agnostic event emission.
///
/// Implementors bridge the generic `(name, json_payload)` call to whatever
/// transport their runtime uses (Tauri IPC, WebSocket broadcast, stdout, etc.).
///
/// All implementations must be `Send + Sync` so they can be shared across
/// async tasks via `Arc<dyn EventEmitter>`.
pub trait EventEmitter: Send + Sync {
    /// Emits a named event with a pre-serialised JSON payload.
    ///
    /// `name` must match a frontend event listener (e.g. `"text-log"`,
    /// `"world-update"`).  `payload` is the serialised form of whichever IPC
    /// type corresponds to that event.
    fn emit_event(&self, name: &str, payload: serde_json::Value);
}
