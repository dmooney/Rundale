//! Configuration types for the Parish game engine.

pub mod builtin_providers;
pub mod engine;
pub mod flags;
pub mod local_dialogue;
pub mod provider;
pub mod user_config;
pub mod v2;

pub use engine::*;
pub use flags::FeatureFlags;
pub use provider::*;
pub use v2::*;

// Re-export SpeedConfig from parish-types so downstream crates can find it
// at `parish_core::config::SpeedConfig`.
pub use parish_types::SpeedConfig;
