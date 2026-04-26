//! Parish core game-logic library.
//!
//! Contains all backend-agnostic game systems: world graph, NPC management,
//! LLM inference pipeline, player input parsing, and persistence.
//! Consumed by the CLI binary (headless), the Tauri desktop frontend,
//! and the axum web server.

// Retained modules — IPC, orchestration glue, and mod loading
pub mod debug_snapshot;
pub mod editor;
pub mod game_mod;
pub mod game_session;
pub mod inference_guard;
pub mod ipc;
pub mod loading;
pub mod prompts;

// Sub-crate re-exports — preserves `crate::X::...` paths used throughout
pub use parish_config as config;
pub use parish_inference as inference;
pub use parish_input as input;
pub use parish_npc as npc;
pub use parish_persistence as persistence;
pub use parish_types::dice;
pub use parish_types::error;
pub use parish_world as world;
