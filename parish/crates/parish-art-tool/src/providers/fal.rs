//! fal.ai image generation provider (FLUX.1 [dev] / [schnell]).
//!
//! API key from `FAL_KEY`. The cheapest top-tier option: FLUX.1 [schnell] is
//! ~$0.003/image, [dev] ~$0.025.
//!
//! Model: `fal-ai/flux/dev` via `POST https://fal.run/fal-ai/flux/dev`.
//!
//! # Prompt size limit (AGENTS rule 16)
//!
//! FLUX uses a T5 text encoder with a 512-token context (~2000 chars); fal does
//! not document a hard character cap, so we apply a conservative 2000-char
//! limit (budget 1800) client-side and truncate with a `[truncated N chars]`
//! marker.
//!
//! NOTE: The request shape follows the fal FLUX schema (`prompt`, `image_size`
//! object, `num_images`, `num_inference_steps`). The response carries image
//! **URLs** (`images[].url`), not base64 — `generate` downloads each. Field
//! names follow the published fal schema; drift is marked so it is visible at
//! review. FLUX has no transparent-background mode; `ImageBackground` is
//! ignored (sprite cut-out happens in post-processing).

use std::path::Path;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use super::{ImageBackground, ImageProvider, ImageSize, apply_prompt_budget};

/// Conservative prompt-length limit (chars) — FLUX T5 context ≈ 512 tokens.
/// NOTE: fal does not document a hard cap; this is a safety budget.
pub const FAL_PROMPT_LIMIT: usize = 2_000;

/// Client-side budget = 90% of the limit. Asserted in the tests.
pub const FAL_PROMPT_BUDGET: usize = (FAL_PROMPT_LIMIT as f64 * 0.90) as usize;

/// Explicit pixel dimensions for the fal `image_size` object.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FalImageSize {
    pub width: u32,
    pub height: u32,
}

/// Request body for the fal FLUX endpoint.
///
/// NOTE: mirrors the published fal FLUX schema.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FalGenerateRequest {
    pub prompt: String,
    pub image_size: FalImageSize,
    pub num_images: u32,
    pub num_inference_steps: u32,
    pub enable_safety_checker: bool,
}

/// Response body — generated images carried as URLs.
#[derive(Debug, Deserialize)]
pub struct FalGenerateResponse {
    pub images: Vec<FalImage>,
}

/// One generated image reference.
#[derive(Debug, Deserialize)]
pub struct FalImage {
    pub url: String,
}

/// fal.ai image provider.
pub struct FalProvider {
    api_key: String,
    endpoint: String,
}

impl FalProvider {
    /// Create a provider, reading the key from `FAL_KEY`.
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("FAL_KEY").map_err(|_| anyhow::anyhow!("FAL_KEY not set"))?;
        Ok(Self {
            api_key,
            endpoint: "https://fal.run/fal-ai/flux/dev".to_string(),
        })
    }

    /// Build a generate request body (enforces prompt budget per rule 16).
    ///
    /// `_background` is ignored — FLUX has no transparent-background mode.
    pub fn build_generate_request(
        prompt: &str,
        n: u32,
        size: ImageSize,
        _background: ImageBackground,
    ) -> FalGenerateRequest {
        let safe_prompt = apply_prompt_budget(prompt, FAL_PROMPT_BUDGET);
        let (width, height) = size.dimensions();
        FalGenerateRequest {
            prompt: safe_prompt,
            image_size: FalImageSize { width, height },
            num_images: n.max(1),
            num_inference_steps: 28,
            enable_safety_checker: true,
        }
    }
}

impl ImageProvider for FalProvider {
    fn generate(
        &self,
        prompt: &str,
        n: u32,
        size: ImageSize,
        background: ImageBackground,
    ) -> Result<Vec<Vec<u8>>> {
        let request = Self::build_generate_request(prompt, n, size, background);
        let body = serde_json::to_string(&request)?;
        let client = reqwest::blocking::Client::new();
        let resp = client
            .post(&self.endpoint)
            .header("Authorization", format!("Key {}", self.api_key))
            .header("Content-Type", "application/json")
            .body(body)
            .send()?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            bail!("fal API error {status}: {text}");
        }
        let response: FalGenerateResponse = resp.json()?;
        // fal returns image URLs — download each into bytes.
        let mut images = Vec::new();
        for img in response.images {
            let bytes = client.get(&img.url).send()?.bytes()?.to_vec();
            images.push(bytes);
        }
        Ok(images)
    }

    fn edit(
        &self,
        _prompt: &str,
        _reference_paths: &[&Path],
        _size: ImageSize,
    ) -> Result<Vec<Vec<u8>>> {
        bail!("fal img2img edit not implemented — use the flux/dev/image-to-image route in M5")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_body_serde_roundtrip() {
        let req = FalProvider::build_generate_request(
            "A pixel art plate.",
            3,
            ImageSize::Landscape1536x1024,
            ImageBackground::Opaque,
        );
        let json = serde_json::to_string(&req).unwrap();
        let back: FalGenerateRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, back);
    }

    #[test]
    fn plate_uses_landscape_dimensions_and_num_images() {
        let req = FalProvider::build_generate_request(
            "plate",
            3,
            ImageSize::Landscape1536x1024,
            ImageBackground::Opaque,
        );
        assert_eq!((req.image_size.width, req.image_size.height), (1536, 1024));
        assert_eq!(req.num_images, 3);
    }

    #[test]
    fn sprite_uses_square_dimensions() {
        let req = FalProvider::build_generate_request(
            "sprite",
            1,
            ImageSize::Square1024x1024,
            ImageBackground::Transparent,
        );
        assert_eq!((req.image_size.width, req.image_size.height), (1024, 1024));
    }

    /// AGENTS rule 16: prompt field length <= FAL_PROMPT_BUDGET.
    #[test]
    fn prompt_field_len_within_budget() {
        let long = "x".repeat(FAL_PROMPT_LIMIT + 500);
        let req = FalProvider::build_generate_request(
            &long,
            1,
            ImageSize::Landscape1536x1024,
            ImageBackground::Opaque,
        );
        assert!(
            req.prompt.len() <= FAL_PROMPT_BUDGET,
            "prompt.len()={} > budget={FAL_PROMPT_BUDGET}",
            req.prompt.len()
        );
        assert!(req.prompt.starts_with("[truncated "));
    }

    #[test]
    fn short_prompt_not_truncated() {
        let req = FalProvider::build_generate_request(
            "short",
            1,
            ImageSize::Square1024x1024,
            ImageBackground::Opaque,
        );
        assert_eq!(req.prompt, "short");
    }

    #[test]
    fn request_json_has_required_fields() {
        let req = FalProvider::build_generate_request(
            "Test",
            2,
            ImageSize::Landscape1536x1024,
            ImageBackground::Opaque,
        );
        let v: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&req).unwrap()).unwrap();
        assert!(v["prompt"].is_string());
        assert!(v["image_size"]["width"].is_number());
        assert!(v["num_images"].is_number());
        assert!(v["num_inference_steps"].is_number());
    }
}
