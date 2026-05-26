//! Debug snapshot — serializable aggregate of all game state for debug UIs.
//!
//! Provides a single `DebugSnapshot` struct that captures a point-in-time
//! view of all inspectable game internals. Consumed by both the TUI debug
//! panel and the Tauri/Svelte debug panel via IPC.

pub(crate) mod types;
pub(crate) mod build;
mod reexport;

pub use types::*;
pub use build::{
    build_configured_providers, build_debug_snapshot, build_inference_categories,
};
pub use reexport::*;

#[cfg(test)]
mod tests;
