//! Configuration types for the Parish game engine.

pub mod engine;
pub mod provider;

pub use engine::*;
pub use provider::*;

// Re-export SpeedConfig from parish-types so downstream crates can find it
// at `parish_core::config::SpeedConfig`.
pub use parish_types::SpeedConfig;
