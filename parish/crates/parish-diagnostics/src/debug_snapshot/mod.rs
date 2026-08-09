//! Debug snapshot — serializable aggregate of all game state for debug UIs.
//!
//! Provides a single `DebugSnapshot` struct that captures a point-in-time
//! view of all inspectable game internals. Consumed by both the TUI debug
//! panel and the Tauri/Svelte debug panel via IPC.

use parish_config::InferenceCategory;

pub(crate) mod build;
mod reexport;
pub(crate) mod types;

pub use build::{build_configured_providers, build_debug_snapshot, build_inference_categories};
pub use reexport::*;
pub use types::*;

/// Per-category inference-config view consumed by [`build_inference_categories`].
///
/// `parish-diagnostics` is a backend-agnostic leaf crate and cannot depend on
/// `parish-core` (where the concrete `GameConfig` lives) without a dependency
/// cycle. This trait is the seam: `parish-core` implements it for its
/// `GameConfig` so the builder reads the per-role provider/model/base-url
/// overrides without reaching back into the orchestration crate.
pub trait InferenceCategoryConfig {
    /// Provider override for `cat`, or `None` to inherit the base provider.
    fn category_provider(&self, cat: InferenceCategory) -> Option<String>;
    /// Model override for `cat`, or `None` to inherit the base model.
    fn category_model(&self, cat: InferenceCategory) -> Option<String>;
    /// Base-URL override for `cat`, or `None` to inherit the base URL.
    fn category_base_url(&self, cat: InferenceCategory) -> Option<String>;
    /// Effective profile after top-level and category overrides for the exact
    /// audited workload (Tier 2 and Tier 3 intentionally have separate caps).
    fn subrole_profile(
        &self,
        subrole: parish_config::InferenceSubrole,
    ) -> parish_config::InferenceProfile;
}

#[cfg(test)]
mod tests;
