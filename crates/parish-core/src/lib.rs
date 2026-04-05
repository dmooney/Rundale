//! Parish core game-logic library.
//!
//! Contains all backend-agnostic game systems: world graph, NPC management,
//! LLM inference pipeline, player input parsing, and persistence.
//! Consumed by the CLI binary (headless), the Tauri desktop frontend,
//! and the axum web server.

pub mod backend_init;
pub mod config;
pub mod debug_snapshot;
pub mod dice;
pub mod error;
pub mod game_mod;
pub mod game_session;
pub mod inference;
pub mod input;
pub mod ipc;
pub mod loading;
pub mod npc;
pub mod persistence;
pub mod world;
