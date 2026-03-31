//! IPC type definitions and handler logic shared by all frontends.
//!
//! Contains the serializable types exchanged between the game engine and
//! any UI layer (Tauri desktop, axum web server, etc.), plus pure functions
//! that build those types from game state.

pub mod handlers;
pub mod types;

pub use handlers::*;
pub use types::*;
