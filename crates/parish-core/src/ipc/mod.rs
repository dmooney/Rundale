//! IPC type definitions and handler logic shared by all frontends.
//!
//! Contains the serializable types exchanged between the game engine and
//! any UI layer (Tauri desktop, axum web server, etc.), plus pure functions
//! that build those types from game state.

pub mod commands;
pub mod config;
pub mod editor;
pub mod handlers;
pub mod streaming;
pub mod types;

pub use commands::{
    CommandEffect, CommandResult, TextPresentation, handle_command, render_look_text,
};
pub use config::GameConfig;
pub use handlers::*;
pub use streaming::stream_npc_tokens;
pub use types::*;
