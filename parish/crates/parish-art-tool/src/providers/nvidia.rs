//! NVIDIA NIM image generation provider (FLUX.1-dev).
//!
//! API key from `NVIDIA_API_KEY`. This is the cheapest option to *try* in this
//! repo because the key is already present in `.env`.
//!
//! Model: `black-forest-labs/flux.1-dev` via the NVIDIA NIM genai endpoint
//! `POST https://ai.api.nvidia.com/v1/genai/black-forest-labs/flux.1-dev`.
//!
//! # Prompt size limit (AGENTS rule 16)
//!
//! FLUX uses a T5 text encoder with a 512-token context (~2000 chars); the NIM
//! endpoint does not document a hard character cap, so we apply a conservative
//! 2000-char limit (budget 1800) client-side and truncate with a
//! `[truncated N chars]` marker.
//!
//! NOTE: The request/response shapes follow the NVIDIA NIM FLUX.1-dev reference
//! (width/height/steps/cfg_scale body; `artifacts[].base64` response). If the
//! NIM schema changes, update these structs — they are marked so the
//! discrepancy is visible at review time. FLUX has no native transparent
//! background, so `ImageBackground` is ignored here (sprite cut-out happens in
//! post-processing).

use std::path::Path;

use anyhow::{Result, bail};
use base64::Engine;
use serde::{Deserialize, Serialize};

use super::{ImageBackground, ImageProvider, ImageSize, apply_prompt_budget};

/// Conservative prompt-length limit (chars) — FLUX T5 context ≈ 512 tokens.
/// NOTE: NVIDIA NIM does not document a hard cap; this is a safety budget.
pub const NVIDIA_PROMPT_LIMIT: usize = 2_000;

/// Client-side budget = 90% of the limit. A `payload.len() <= NVIDIA_PROMPT_BUDGET`
/// assertion lives in the tests.
pub const NVIDIA_PROMPT_BUDGET: usize = (NVIDIA_PROMPT_LIMIT as f64 * 0.90) as usize;

/// Request body for the NVIDIA NIM FLUX.1-dev genai endpoint.
///
/// NOTE: mirrors the documented NIM FLUX.1-dev invoke schema.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NvidiaGenerateRequest {
    pub prompt: String,
    /// "base" for text-to-image.
    pub mode: String,
    pub cfg_scale: f32,
    pub width: u32,
    pub height: u32,
    pub seed: u32,
    pub steps: u32,
}

/// Response body — one or more artifacts carrying base64 PNG bytes.
#[derive(Debug, Deserialize)]
pub struct NvidiaGenerateResponse {
    pub artifacts: Vec<NvidiaArtifact>,
}

/// One generated artifact.
#[derive(Debug, Deserialize)]
pub struct NvidiaArtifact {
    pub base64: Option<String>,
}

/// NVIDIA NIM image provider.
pub struct NvidiaProvider {
    api_key: String,
    base_url: String,
    model: String,
}

impl NvidiaProvider {
    /// Create a provider, reading the key from `NVIDIA_API_KEY`.
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("NVIDIA_API_KEY")
            .map_err(|_| anyhow::anyhow!("NVIDIA_API_KEY not set"))?;
        Ok(Self {
            api_key,
            base_url: "https://ai.api.nvidia.com/v1/genai".to_string(),
            model: "black-forest-labs/flux.1-dev".to_string(),
        })
    }

    /// Build a generate request body (enforces prompt budget per rule 16).
    ///
    /// `_background` is ignored — FLUX has no transparent-background mode; sprite
    /// cut-out is handled in post-processing. `n` is not a NIM body field (FLUX
    /// returns one image per call); the caller loops for multiple candidates.
    pub fn build_generate_request(
        prompt: &str,
        size: ImageSize,
        _background: ImageBackground,
    ) -> NvidiaGenerateRequest {
        let safe_prompt = apply_prompt_budget(prompt, NVIDIA_PROMPT_BUDGET);
        let (width, height) = size.dimensions();
        NvidiaGenerateRequest {
            prompt: safe_prompt,
            mode: "base".to_string(),
            cfg_scale: 3.5,
            width,
            height,
            seed: 0,
            steps: 50,
        }
    }
}

impl ImageProvider for NvidiaProvider {
    fn generate(
        &self,
        prompt: &str,
        n: u32,
        size: ImageSize,
        background: ImageBackground,
    ) -> Result<Vec<Vec<u8>>> {
        // FLUX NIM returns one image per call; loop for n candidates.
        let mut images = Vec::new();
        for _ in 0..n.max(1) {
            let request = Self::build_generate_request(prompt, size, background);
            let body = serde_json::to_string(&request)?;
            let client = reqwest::blocking::Client::new();
            let resp = client
                .post(format!("{}/{}", self.base_url, self.model))
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .header("Accept", "application/json")
                .body(body)
                .send()?;
            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().unwrap_or_default();
                bail!("NVIDIA NIM API error {status}: {text}");
            }
            let response: NvidiaGenerateResponse = resp.json()?;
            for art in response.artifacts {
                if let Some(b64) = art.base64 {
                    images.push(base64::engine::general_purpose::STANDARD.decode(&b64)?);
                }
            }
        }
        Ok(images)
    }

    fn edit(
        &self,
        _prompt: &str,
        _reference_paths: &[&Path],
        _size: ImageSize,
    ) -> Result<Vec<Vec<u8>>> {
        bail!("NVIDIA NIM img2img edit not implemented — use gen with anchor prompt")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_body_serde_roundtrip() {
        let req = NvidiaProvider::build_generate_request(
            "A pixel art plate of Darcy's Pub.",
            ImageSize::Landscape1536x1024,
            ImageBackground::Opaque,
        );
        let json = serde_json::to_string(&req).unwrap();
        let back: NvidiaGenerateRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, back);
    }

    #[test]
    fn plate_uses_landscape_dimensions() {
        let req = NvidiaProvider::build_generate_request(
            "plate",
            ImageSize::Landscape1536x1024,
            ImageBackground::Opaque,
        );
        assert_eq!((req.width, req.height), (1536, 1024));
        assert_eq!(req.mode, "base");
    }

    #[test]
    fn sprite_uses_square_dimensions() {
        let req = NvidiaProvider::build_generate_request(
            "sprite",
            ImageSize::Square1024x1024,
            ImageBackground::Transparent,
        );
        assert_eq!((req.width, req.height), (1024, 1024));
    }

    /// AGENTS rule 16: prompt field length <= NVIDIA_PROMPT_BUDGET.
    #[test]
    fn prompt_field_len_within_budget() {
        let long = "x".repeat(NVIDIA_PROMPT_LIMIT + 500);
        let req = NvidiaProvider::build_generate_request(
            &long,
            ImageSize::Landscape1536x1024,
            ImageBackground::Opaque,
        );
        assert!(
            req.prompt.len() <= NVIDIA_PROMPT_BUDGET,
            "prompt.len()={} > budget={NVIDIA_PROMPT_BUDGET}",
            req.prompt.len()
        );
        assert!(req.prompt.starts_with("[truncated "));
    }

    #[test]
    fn short_prompt_not_truncated() {
        let req = NvidiaProvider::build_generate_request(
            "short",
            ImageSize::Square1024x1024,
            ImageBackground::Opaque,
        );
        assert_eq!(req.prompt, "short");
    }

    #[test]
    fn request_json_has_required_fields() {
        let req = NvidiaProvider::build_generate_request(
            "Test",
            ImageSize::Landscape1536x1024,
            ImageBackground::Opaque,
        );
        let v: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&req).unwrap()).unwrap();
        for k in ["prompt", "mode", "cfg_scale", "width", "height", "steps"] {
            assert!(v.get(k).is_some(), "missing field {k}");
        }
    }
}
