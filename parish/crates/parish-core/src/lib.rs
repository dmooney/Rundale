//! Parish orchestration layer.
//!
//! Composes backend-agnostic leaf crates (`parish-world`, `parish-npc`,
//! `parish-inference`, `parish-input`, `parish-persistence`) into shared
//! game-loop, IPC, mod-loading, and session-management logic.
//! Consumed by the CLI binary (headless), the Tauri desktop frontend,
//! and the axum web server. Leaf-crate ownership lives in the respective
//! crates under `parish/crates/`.

// Retained modules — IPC, orchestration glue, and mod loading. These are the
// legacy desktop composition surface; mobile gets the leaf aliases below and
// adds its own portable worker boundary in a separate module.
// The on-disk chronicle writers (per-character / per-location markdown logs
// and the JSONL chat transcript) were extracted into their own crate
// (`parish-chronicle`). These re-exports preserve the historical
// `parish_core::{character_log, chat_transcript, location_log}::...` paths for
// every consumer (server `session`/`state`, Tauri `setup`, engine `app`) so
// the extraction stays behaviour-preserving with zero import changes. The
// branch-switch subscriber-rebind call sites stay in their entry-point crates
// and reach the managers through these re-exports.
#[cfg(feature = "desktop")]
pub use parish_chronicle::character_log;
#[cfg(feature = "desktop")]
pub use parish_chronicle::chat_transcript;
#[cfg(feature = "desktop")]
pub use parish_chronicle::location_log;
// The debug-snapshot builders and bug-report orchestration were extracted into
// their own crate (`parish-diagnostics`). This re-export preserves the
// historical `parish_core::debug_snapshot::...` path for every consumer
// (`parish-tauri`, `parish-server`, `parish-engine`, tests) so the extraction
// stays behaviour-preserving with zero import changes. The `bug_report` shim
// lives in `crate::ipc` to preserve `parish_core::ipc::bug_report::...`.
#[cfg(feature = "desktop")]
pub use parish_diagnostics::debug_snapshot;
// The Parish Designer backend was extracted into its own crate
// (`parish-editor`). This re-export preserves the historical
// `parish_core::editor::...` path for every consumer (`parish-tauri`,
// `parish-server`, `crate::ipc::editor`) so the extraction stays
// behaviour-preserving with zero import changes.
#[cfg(feature = "desktop")]
pub use parish_editor as editor;
// Portable state seams remain available when the desktop composition is
// disabled. They deliberately avoid the IPC and inference orchestration tree.
pub mod dialogue_apply;
#[cfg(feature = "desktop")]
pub mod event_bus;
#[cfg(feature = "desktop")]
pub mod game_loop;
#[cfg(feature = "desktop")]
pub mod game_session;
#[cfg(feature = "desktop")]
pub mod identity;
#[cfg(feature = "desktop")]
pub mod inference_guard;
#[cfg(feature = "desktop")]
pub mod inference_runtime_v2;
#[cfg(feature = "desktop")]
pub mod ipc;
#[cfg(feature = "desktop")]
pub mod loading;
#[cfg(feature = "mobile")]
pub mod mobile;
#[cfg(feature = "desktop")]
pub mod mod_source;
pub mod portable_look;
#[cfg(feature = "desktop")]
pub mod prompts;
#[cfg(feature = "desktop")]
pub mod secret_store;
#[cfg(feature = "desktop")]
pub mod session_store;
#[cfg(feature = "desktop")]
pub mod tile_cache;

/// How often autosave tasks should snapshot active sessions (seconds).
/// Used by both the Axum web server and the Tauri desktop backend.
/// Changing this risks silent data loss on crash — update tests accordingly.
#[cfg(feature = "desktop")]
pub const AUTOSAVE_INTERVAL_SECS: u64 = 60;

// Sub-crate re-exports — preserves `crate::X::...` paths used throughout
pub use parish_config as config;
// The content-mod loader was extracted into its own crate (`parish-mod`).
// This alias preserves the historical `parish_core::game_mod::...` path for
// every consumer so the extraction stays behaviour-preserving.
pub use parish_inference as inference;
pub use parish_input as input;
#[cfg(feature = "desktop")]
pub use parish_mod as game_mod;
pub use parish_npc as npc;
pub use parish_persistence as persistence;
pub use parish_types as types;
pub use parish_types::ReactionDirection;
pub use parish_types::dice;
pub use parish_types::error;
pub use parish_world as world;
