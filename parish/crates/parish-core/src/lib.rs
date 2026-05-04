//! Parish core game-logic library.
//!
//! Contains all backend-agnostic game systems: world graph, NPC management,
//! LLM inference pipeline, player input parsing, and persistence.
//! Consumed by the CLI binary (headless), the Tauri desktop frontend,
//! and the axum web server.

// Retained modules — IPC, orchestration glue, and mod loading
pub mod debug_snapshot;
pub mod editor;
pub mod event_bus;
pub mod game_mod;
pub mod game_session;
pub mod inference_guard;
pub mod ipc;
pub mod loading;
pub mod mod_source;
pub mod prompts;
pub mod tile_cache;

/// How often autosave tasks should snapshot active sessions (seconds).
/// Used by both the Axum web server and the Tauri desktop backend.
/// Changing this risks silent data loss on crash — update tests accordingly.
pub const AUTOSAVE_INTERVAL_SECS: u64 = 60;

// Sub-crate re-exports — preserves `crate::X::...` paths used throughout
pub use parish_config as config;
pub use parish_inference as inference;
pub use parish_input as input;
pub use parish_npc as npc;
pub use parish_persistence as persistence;
pub use parish_types::dice;
pub use parish_types::error;
pub use parish_world as world;
