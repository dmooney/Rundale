use serde::{Deserialize, Serialize};

/// Parchment pipeline tuning parameters — exposed on `ArtConfig::Parchment`
/// and as the fallback inside `ArtConfig::Diffusion`.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ParchmentConfig {
    /// Strength of the Sobel ink overlay (0.0 = off, 1.0 = full black).
    #[serde(default = "default_ink_strength")]
    pub ink_strength: f32,
    /// Strength of the Perlin paper-grain overlay (0.0 = off, 1.0 = heavy).
    #[serde(default = "default_grain_strength")]
    pub grain_strength: f32,
    /// Box-blur radius applied before edge detection (0 = no smoothing, 1 = 3×3).
    #[serde(default = "default_smooth_radius")]
    pub smooth_radius: u32,
}

impl Default for ParchmentConfig {
    fn default() -> Self {
        Self {
            ink_strength: default_ink_strength(),
            grain_strength: default_grain_strength(),
            smooth_radius: default_smooth_radius(),
        }
    }
}

fn default_ink_strength() -> f32 {
    0.40
}
fn default_grain_strength() -> f32 {
    0.07
}
fn default_smooth_radius() -> u32 {
    1
}

/// Art-pipeline configuration — selects which backend to use when pre-generating
/// stylized tiles with `parish-geo-tool seed-tiles`.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ArtConfig {
    /// Deterministic CPU-only parchment pipeline.  Always available offline.
    Parchment(ParchmentConfig),
    /// AI img2img via a local Stable Diffusion WebUI (A1111-compatible).
    /// Falls back to `fallback` automatically when the endpoint is unreachable.
    Diffusion {
        /// SD WebUI base URL, e.g. `"http://localhost:7860"`.
        endpoint: String,
        /// Optional checkpoint name; omit to use SD WebUI's currently loaded model.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        /// img2img prompt sent to the SD API.
        #[serde(default = "default_diffusion_prompt")]
        prompt: String,
        /// Denoising strength (0.0–1.0). 0.65 preserves structure while allowing
        /// strong stylization.
        #[serde(default = "default_denoising")]
        denoising_strength: f32,
        /// ControlNet weight for the Canny hint (0.0–1.0). 0.85 keeps geographic
        /// features recognisable while allowing painterly reinterpretation.
        #[serde(default = "default_controlnet")]
        controlnet_strength: f32,
        /// Parchment pipeline used when the SD endpoint is unreachable.
        #[serde(default)]
        fallback: ParchmentConfig,
    },
}

fn default_diffusion_prompt() -> String {
    default_diffusion_prompt_pub()
}

/// Default SD img2img prompt — also used by the `seed-tiles` CLI.
pub fn default_diffusion_prompt_pub() -> String {
    "historical hand-drawn map, parchment paper, County Roscommon Ireland 1820, \
     sepia ink illustration, old cartography style, watercolor tints, aged paper \
     texture, illustrated like Red Dead Redemption 2 map"
        .to_string()
}
fn default_denoising() -> f32 {
    0.65
}
fn default_controlnet() -> f32 {
    0.85
}
