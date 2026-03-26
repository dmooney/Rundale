//! Parish core game-logic library.
//!
//! Contains all backend-agnostic game systems: world graph, NPC management,
//! LLM inference pipeline, player input parsing, and persistence.
//! Consumed by the CLI binary (headless) and the Tauri desktop frontend.

pub mod config;
pub mod error;
pub mod inference;
pub mod input;
pub mod loading;
pub mod npc;
pub mod persistence;
pub mod world;
