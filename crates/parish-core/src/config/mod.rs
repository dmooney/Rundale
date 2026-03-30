//! Configuration module for provider settings and engine tuning.
//!
//! Provider config (`provider.rs`) handles LLM backend selection.
//! Engine config (`engine.rs`) contains all tunable engine parameters
//! that can be overridden via `parish.toml`.

mod engine;
mod provider;

pub use engine::*;
pub use provider::*;
