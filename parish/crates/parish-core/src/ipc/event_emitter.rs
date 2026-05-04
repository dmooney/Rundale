//! Abstract event-emission trait shared by all Parish backends.
//!
//! The three backends emit events differently:
//!
//! - `parish-tauri` uses `tauri::AppHandle::emit(name, payload)`.
//! - `parish-server` uses `EventBus::emit(name, &payload)` which serialises
//!   via `serde_json::to_value` and broadcasts to WebSocket clients.
//! - `parish-cli` headless mode currently logs to stdout; it will implement
//!   a no-op or println-backed emitter in a future slice.
//!
//! This trait is **object-safe** (no generic methods) so the next refactor
//! slice can pass `Arc<dyn EventEmitter>` into shared game-loop helpers without
//! duplicating `handle_game_input`, `run_npc_turn`, etc.  Serialisation is the
//! caller's responsibility: callers should convert their typed payload to
//! `serde_json::Value` before calling `emit_event`.
//!
//! # Next-slice migration notes (#696)
//!
//! When the shared game-loop functions are extracted in the next slice, each
//! backend will wrap its native emitter in a thin newtype that implements this
//! trait:
//!
//! - Tauri: `struct TauriEmitter(tauri::AppHandle)` → `emit_event` calls
//!   `self.0.emit(name, payload)` after deserialising back to the concrete type,
//!   or by storing the raw `Value`.
//! - Server: `struct BusEmitter(Arc<EventBus>)` → `emit_event` calls
//!   `self.0.send(ServerEvent { event: name.to_string(), payload })`.
//!
//! The trait intentionally does **not** mirror the concrete backend signatures
//! exactly — that alignment happens in the per-backend impl blocks.

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
