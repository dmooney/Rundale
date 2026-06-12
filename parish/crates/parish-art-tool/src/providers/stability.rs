//! Stability AI image generation provider (Stable Diffusion 3.5).
//!
//! API key from `STABILITY_API_KEY`.
//!
//! Model: `sd3.5-large` via the v2beta stable-image endpoint
//! `POST https://api.stability.ai/v2beta/stable-image/generate/sd3`.
//!
//! # Prompt size limit (AGENTS rule 16)
//!
//! The Stability v2beta `prompt` field documents a maximum of **10 000
//! characters**. We enforce 90% of that (9000) client-side and truncate with a
//! `[truncated N chars]` marker.
//!
//! NOTE: The v2beta stable-image endpoint takes `multipart/form-data` (not
//! JSON) and returns raw image bytes when `Accept: image/*`. The logical field
//! set is modelled in [`StabilityGenerateRequest`] for offline testing and
//! converted to a multipart form in `generate`. Field names follow the
//! documented v2beta schema; any drift is marked so it is visible at review.

use std::path::Path;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use super::{ImageBackground, ImageProvider, ImageSize, apply_prompt_budget};

/// Stability v2beta documented maximum prompt length (characters).
/// NOTE: Source — Stability v2beta stable-image generate reference.
pub const STABILITY_PROMPT_LIMIT: usize = 10_000;

/// Client-side budget = 90% of the limit. Asserted in the tests.
pub const STABILITY_PROMPT_BUDGET: usize = (STABILITY_PROMPT_LIMIT as f64 * 0.90) as usize;

/// Logical field set for the v2beta stable-image generate request.
///
/// NOTE: sent as `multipart/form-data`, modelled here for offline assertion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StabilityGenerateRequest {
    pub prompt: String,
    pub model: String,
    /// Aspect ratio, e.g. "16:9" (plate) or "1:1" (sprite).
    pub aspect_ratio: String,
    /// Output format — "png".
    pub output_format: String,
}

/// Stability AI image provider.
pub struct StabilityProvider {
    api_key: String,
    base_url: String,
    model: String,
}

impl StabilityProvider {
    /// Create a provider, reading the key from `STABILITY_API_KEY`.
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("STABILITY_API_KEY")
            .map_err(|_| anyhow::anyhow!("STABILITY_API_KEY not set"))?;
        Ok(Self {
            api_key,
            base_url: "https://api.stability.ai/v2beta/stable-image/generate".to_string(),
            model: "sd3.5-large".to_string(),
        })
    }

    /// Build the logical request fields (enforces prompt budget per rule 16).
    ///
    /// `_background` is ignored — SD3.5 has no transparent-background mode;
    /// sprite cut-out is handled in post-processing. `n` is not a v2beta field
    /// (one image per call); the caller loops for multiple candidates.
    pub fn build_generate_request(
        prompt: &str,
        size: ImageSize,
        _background: ImageBackground,
    ) -> StabilityGenerateRequest {
        let safe_prompt = apply_prompt_budget(prompt, STABILITY_PROMPT_BUDGET);
        StabilityGenerateRequest {
            prompt: safe_prompt,
            model: "sd3.5-large".to_string(),
            aspect_ratio: size.aspect_ratio().to_string(),
            output_format: "png".to_string(),
        }
    }
}

impl ImageProvider for StabilityProvider {
    fn generate(
        &self,
        prompt: &str,
        n: u32,
        size: ImageSize,
        background: ImageBackground,
    ) -> Result<Vec<Vec<u8>>> {
        // v2beta returns one image per call; loop for n candidates.
        let mut images = Vec::new();
        for _ in 0..n.max(1) {
            let req = Self::build_generate_request(prompt, size, background);
            let form = reqwest::blocking::multipart::Form::new()
                .text("prompt", req.prompt)
                .text("model", req.model)
                .text("aspect_ratio", req.aspect_ratio)
                .text("output_format", req.output_format);
            let client = reqwest::blocking::Client::new();
            let resp = client
                .post(format!("{}/sd3", self.base_url))
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Accept", "image/*")
                .multipart(form)
                .send()?;
            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().unwrap_or_default();
                bail!("Stability API error {status}: {text}");
            }
            // Accept: image/* → the body IS the PNG.
            images.push(resp.bytes()?.to_vec());
        }
        Ok(images)
    }

    fn edit(
        &self,
        _prompt: &str,
        _reference_paths: &[&Path],
        _size: ImageSize,
    ) -> Result<Vec<Vec<u8>>> {
        bail!("Stability img2img edit not implemented — use gen with anchor prompt")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_body_serde_roundtrip() {
        let req = StabilityProvider::build_generate_request(
            "A pixel art plate.",
            ImageSize::Landscape1536x1024,
            ImageBackground::Opaque,
        );
        let json = serde_json::to_string(&req).unwrap();
        let back: StabilityGenerateRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, back);
    }

    #[test]
    fn plate_uses_16_9_aspect() {
        let req = StabilityProvider::build_generate_request(
            "plate",
            ImageSize::Landscape1536x1024,
            ImageBackground::Opaque,
        );
        assert_eq!(req.aspect_ratio, "16:9");
        assert_eq!(req.model, "sd3.5-large");
        assert_eq!(req.output_format, "png");
    }

    #[test]
    fn sprite_uses_1_1_aspect() {
        let req = StabilityProvider::build_generate_request(
            "sprite",
            ImageSize::Square1024x1024,
            ImageBackground::Transparent,
        );
        assert_eq!(req.aspect_ratio, "1:1");
    }

    /// AGENTS rule 16: prompt field length <= STABILITY_PROMPT_BUDGET.
    #[test]
    fn prompt_field_len_within_budget() {
        let long = "x".repeat(STABILITY_PROMPT_LIMIT + 500);
        let req = StabilityProvider::build_generate_request(
            &long,
            ImageSize::Landscape1536x1024,
            ImageBackground::Opaque,
        );
        assert!(
            req.prompt.len() <= STABILITY_PROMPT_BUDGET,
            "prompt.len()={} > budget={STABILITY_PROMPT_BUDGET}",
            req.prompt.len()
        );
        assert!(req.prompt.starts_with("[truncated "));
    }

    #[test]
    fn short_prompt_not_truncated() {
        let req = StabilityProvider::build_generate_request(
            "short",
            ImageSize::Square1024x1024,
            ImageBackground::Opaque,
        );
        assert_eq!(req.prompt, "short");
    }

    #[test]
    fn request_json_has_required_fields() {
        let req = StabilityProvider::build_generate_request(
            "Test",
            ImageSize::Landscape1536x1024,
            ImageBackground::Opaque,
        );
        let v: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&req).unwrap()).unwrap();
        for k in ["prompt", "model", "aspect_ratio", "output_format"] {
            assert!(v.get(k).is_some(), "missing field {k}");
        }
    }
}
