//! Limerick orchestration layer.
//!
//! Composes backend-agnostic leaf crates (`limerick-world`, `limerick-npc`,
//! `limerick-inference`, `limerick-input`, `limerick-persistence`) into shared
//! game-loop, IPC, mod-loading, and session-management logic.
//! Consumed by the CLI binary (headless), the Tauri desktop frontend,
//! and the axum web server. Leaf-crate ownership lives in the respective
//! crates under `limerick/crates/`.

// Retained modules — IPC, orchestration glue, and mod loading
// The on-disk chronicle writers (per-character / per-location markdown logs
// and the JSONL chat transcript) were extracted into their own crate
// (`limerick-chronicle`). These re-exports preserve the historical
// `limerick_core::{character_log, chat_transcript, location_log}::...` paths for
// every consumer (server `session`/`state`, Tauri `setup`, engine `app`) so
// the extraction stays behaviour-preserving with zero import changes. The
// branch-switch subscriber-rebind call sites stay in their entry-point crates
// and reach the managers through these re-exports.
pub use limerick_chronicle::character_log;
pub use limerick_chronicle::chat_transcript;
pub use limerick_chronicle::location_log;
// The debug-snapshot builders and bug-report orchestration were extracted into
// their own crate (`limerick-diagnostics`). This re-export preserves the
// historical `limerick_core::debug_snapshot::...` path for every consumer
// (`limerick-tauri`, `limerick-server`, `limerick-engine`, tests) so the extraction
// stays behaviour-preserving with zero import changes. The `bug_report` shim
// lives in `crate::ipc` to preserve `limerick_core::ipc::bug_report::...`.
pub use limerick_diagnostics::debug_snapshot;
// The Limerick Designer backend was extracted into its own crate
// (`limerick-editor`). This re-export preserves the historical
// `limerick_core::editor::...` path for every consumer (`limerick-tauri`,
// `limerick-server`, `crate::ipc::editor`) so the extraction stays
// behaviour-preserving with zero import changes.
pub use limerick_editor as editor;
pub mod event_bus;
pub mod game_loop;
pub mod game_session;
pub mod identity;
pub mod inference_guard;
pub mod inference_runtime_v2;
pub mod ipc;
pub mod loading;
pub mod mod_source;
pub mod prompts;
pub mod secret_store;
pub mod session_store;
pub mod tile_cache;

/// How often autosave tasks should snapshot active sessions (seconds).
/// Used by both the Axum web server and the Tauri desktop backend.
/// Changing this risks silent data loss on crash — update tests accordingly.
pub const AUTOSAVE_INTERVAL_SECS: u64 = 60;

// Sub-crate re-exports — preserves `crate::X::...` paths used throughout
pub use limerick_config as config;
// The content-mod loader was extracted into its own crate (`limerick-mod`).
// This alias preserves the historical `limerick_core::game_mod::...` path for
// every consumer so the extraction stays behaviour-preserving.
pub use limerick_inference as inference;
pub use limerick_input as input;
pub use limerick_mod as game_mod;
pub use limerick_npc as npc;
pub use limerick_persistence as persistence;
pub use limerick_types::ReactionDirection;
pub use limerick_types::dice;
pub use limerick_types::error;
pub use limerick_world as world;
