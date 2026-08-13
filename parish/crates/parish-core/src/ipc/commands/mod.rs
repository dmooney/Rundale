//! Shared system command handler for all Parish backends.
//!
//! [`handle_command`] processes [`Command`] variants against mutable game state
//! and returns a [`CommandResult`] containing the response text and any side
//! effects. Each backend (Tauri, web server, headless CLI, test harness) calls
//! this function after acquiring its own locks, then dispatches the result
//! through its own event/output mechanism.
//!
//! Mode-specific commands (quit, save, load, map, debug, etc.) are returned as
//! [`CommandEffect`] variants so each backend can handle them appropriately.

// ── Sub-modules ───────────────────────────────────────────────────────────────

pub mod dispatch;
pub mod flags;
pub mod help;
pub mod info;
pub mod listen;
pub mod look;
pub mod map;
pub mod provider;
pub mod session;
pub mod theme;
pub mod time;
pub mod toggles;
pub mod types;
pub mod weather;

#[cfg(test)]
mod tests;

// ── Re-exports — public API ───────────────────────────────────────────────────

pub use dispatch::handle_command;
pub use listen::{AtmospherePresentation, render_place_atmosphere};
pub use look::render_look_text;
pub use types::{CommandEffect, CommandResult, TextPresentation};
