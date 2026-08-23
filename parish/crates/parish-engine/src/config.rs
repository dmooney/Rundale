//! Engine-facing configuration types.
//!
//! All inference routing is resolved by the schema-v2 loader in
//! `parish-core::inference_runtime_v2`. The former engine-local TOML/cloud
//! resolver was intentionally removed so it cannot become a second authority.

pub use parish_core::config::{
    CategoryConfig, FeatureFlags, InferenceCategory, InferenceConfig, NpcConfig, Provider,
    ProviderConfig,
};
