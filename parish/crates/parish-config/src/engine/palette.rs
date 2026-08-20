//! Color palette contrast configuration (`[engine.palette]`).

use serde::Deserialize;

/// Color palette contrast configuration.
#[derive(Debug, Deserialize, Clone, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PaletteConfig {
    /// Minimum luminance contrast between foreground and background.
    #[serde(default = "default_min_fg_bg_contrast")]
    pub min_fg_bg_contrast: f32,
    /// Minimum luminance contrast between muted text and background.
    #[serde(default = "default_min_muted_bg_contrast")]
    pub min_muted_bg_contrast: f32,
}

impl Default for PaletteConfig {
    fn default() -> Self {
        Self {
            min_fg_bg_contrast: default_min_fg_bg_contrast(),
            min_muted_bg_contrast: default_min_muted_bg_contrast(),
        }
    }
}

fn default_min_fg_bg_contrast() -> f32 {
    80.0
}
fn default_min_muted_bg_contrast() -> f32 {
    45.0
}
