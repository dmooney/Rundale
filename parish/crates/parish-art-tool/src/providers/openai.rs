//! OpenAI image generation provider.
//!
//! Uses the `gpt-image-1` model via the OpenAI Images API.
//!
//! API key from `OPENAI_API_KEY` environment variable.
//!
//! # Prompt size limit (AGENTS rule 16)
//!
//! The OpenAI `gpt-image-1` generations endpoint documents a maximum prompt
//! length of **32 768 characters** (per the API reference as of 2025-06).
//! We enforce 90% of that (29 491 chars) client-side and truncate with a
//! `[truncated N chars]` marker rather than relying on the provider to return
//! a 4xx.
//!
//! NOTE: The request-body shape is constructed from the OpenAI API reference
//! for `gpt-image-1` (POST /v1/images/generations).  Field names match the
//! documented schema.  If the API changes, update the body structs here —
//! they are clearly documented so the discrepancy is visible at review time.

use std::path::Path;

use anyhow::{Result, bail};
use base64::Engine;
use serde::{Deserialize, Serialize};

use super::{ImageBackground, ImageProvider, ImageSize, apply_prompt_budget};

// ---------------------------------------------------------------------------
// Prompt-size budget (AGENTS rule 16)
// ---------------------------------------------------------------------------

/// OpenAI `gpt-image-1` documented maximum prompt length (characters).
///
/// Source: OpenAI API reference for `/v1/images/generations`, gpt-image-1 model.
/// NOTE: If the provider raises this limit, update the constant and the test.
pub const OPENAI_PROMPT_LIMIT: usize = 32_768;

/// Client-side budget = 90% of the provider limit.
/// A `payload.len() <= OPENAI_PROMPT_BUDGET` assertion lives in the tests.
pub const OPENAI_PROMPT_BUDGET: usize = (OPENAI_PROMPT_LIMIT as f64 * 0.90) as usize;

// ---------------------------------------------------------------------------
// Request-body types
// ---------------------------------------------------------------------------

/// Request body for POST /v1/images/generations (gpt-image-1).
///
/// NOTE: This struct mirrors the documented request schema.  Fields that are
/// optional per the API schema are `Option<T>` here.  No undocumented fields
/// are included.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenAiGenerateRequest {
    /// The image generation model.
    pub model: String,
    /// Text prompt describing the desired image.
    pub prompt: String,
    /// Number of images to generate (1–4 for gpt-image-1).
    pub n: u32,
    /// Output image dimensions.
    pub size: String,
    /// Response format: "url" or "b64_json".
    pub response_format: String,
    /// Background transparency: "opaque" or "transparent".
    /// For sprites we set "transparent"; for plates "opaque".
    /// NOTE: field name per gpt-image-1 API reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
}

/// Response body for POST /v1/images/generations.
#[derive(Debug, Deserialize)]
pub struct OpenAiGenerateResponse {
    pub data: Vec<OpenAiImageData>,
}

/// One generated image.
#[derive(Debug, Deserialize)]
pub struct OpenAiImageData {
    /// Base64-encoded PNG bytes (when `response_format = "b64_json"`).
    pub b64_json: Option<String>,
    /// URL to the generated image (when `response_format = "url"`).
    pub url: Option<String>,
}

/// Request body for POST /v1/images/edits (img2img with reference images).
///
/// NOTE: The edits endpoint takes `multipart/form-data`, not JSON, but we
/// construct the logical field set here and convert to multipart in the
/// provider `edit` implementation.
#[derive(Debug, Clone)]
pub struct OpenAiEditRequest {
    pub model: String,
    pub prompt: String,
    /// Reference images as raw PNG bytes.
    pub reference_images: Vec<Vec<u8>>,
    pub n: u32,
    pub size: String,
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// OpenAI image provider.
pub struct OpenAiProvider {
    api_key: String,
    base_url: String,
}

impl OpenAiProvider {
    /// Create a new provider, reading the key from `OPENAI_API_KEY`.
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| anyhow::anyhow!("OPENAI_API_KEY not set"))?;
        Ok(Self {
            api_key,
            base_url: "https://api.openai.com/v1".to_string(),
        })
    }

    /// Build a generations request body (enforces prompt budget per rule 16).
    pub fn build_generate_request(
        prompt: &str,
        n: u32,
        size: ImageSize,
        background: ImageBackground,
    ) -> OpenAiGenerateRequest {
        let safe_prompt = apply_prompt_budget(prompt, OPENAI_PROMPT_BUDGET);
        OpenAiGenerateRequest {
            model: "gpt-image-1".to_string(),
            prompt: safe_prompt,
            n,
            size: size.as_str().to_string(),
            response_format: "b64_json".to_string(),
            background: match background {
                ImageBackground::Transparent => Some("transparent".to_string()),
                ImageBackground::Opaque => None,
            },
        }
    }
}

impl ImageProvider for OpenAiProvider {
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
            .post(format!("{}/images/generations", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .body(body)
            .send()?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            bail!("OpenAI API error {status}: {text}");
        }

        let response: OpenAiGenerateResponse = resp.json()?;
        let mut images = Vec::new();
        for item in response.data {
            if let Some(b64) = item.b64_json {
                let bytes = base64::engine::general_purpose::STANDARD.decode(&b64)?;
                images.push(bytes);
            }
        }
        Ok(images)
    }

    fn edit(
        &self,
        prompt: &str,
        reference_paths: &[&Path],
        size: ImageSize,
    ) -> Result<Vec<Vec<u8>>> {
        let safe_prompt = apply_prompt_budget(prompt, OPENAI_PROMPT_BUDGET);

        // Build multipart form — image edits use multipart/form-data.
        let mut form = reqwest::blocking::multipart::Form::new()
            .text("model", "gpt-image-1")
            .text("prompt", safe_prompt.clone())
            .text("size", size.as_str().to_string())
            .text("response_format", "b64_json");

        for path in reference_paths {
            let bytes = std::fs::read(path)?;
            let part = reqwest::blocking::multipart::Part::bytes(bytes)
                .file_name("reference.png")
                .mime_str("image/png")?;
            form = form.part("image[]", part);
        }

        let client = reqwest::blocking::Client::new();
        let resp = client
            .post(format!("{}/images/edits", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .multipart(form)
            .send()?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            bail!("OpenAI API error {status}: {text}");
        }

        let response: OpenAiGenerateResponse = resp.json()?;
        let mut images = Vec::new();
        for item in response.data {
            if let Some(b64) = item.b64_json {
                let bytes = base64::engine::general_purpose::STANDARD.decode(&b64)?;
                images.push(bytes);
            }
        }
        Ok(images)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_body_serde_roundtrip() {
        let req = OpenAiProvider::build_generate_request(
            "A pixel art plate of Darcy's Pub.",
            3,
            ImageSize::Landscape1536x1024,
            ImageBackground::Opaque,
        );
        let json = serde_json::to_string(&req).unwrap();
        let roundtripped: OpenAiGenerateRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, roundtripped);
    }

    #[test]
    fn sprite_request_has_transparent_background() {
        let req = OpenAiProvider::build_generate_request(
            "A pixel art sprite of Padraig Darcy.",
            1,
            ImageSize::Square1024x1024,
            ImageBackground::Transparent,
        );
        assert_eq!(req.background.as_deref(), Some("transparent"));
        assert_eq!(req.model, "gpt-image-1");
    }

    #[test]
    fn plate_request_omits_background_field() {
        let req = OpenAiProvider::build_generate_request(
            "A pixel art plate.",
            1,
            ImageSize::Landscape1536x1024,
            ImageBackground::Opaque,
        );
        assert!(req.background.is_none());
    }

    /// AGENTS rule 16: prompt field length <= OPENAI_PROMPT_BUDGET.
    #[test]
    fn prompt_field_len_within_budget() {
        let long_prompt = "x".repeat(OPENAI_PROMPT_LIMIT + 1_000);
        let req = OpenAiProvider::build_generate_request(
            &long_prompt,
            1,
            ImageSize::Landscape1536x1024,
            ImageBackground::Opaque,
        );
        assert!(
            req.prompt.len() <= OPENAI_PROMPT_BUDGET,
            "prompt.len()={} > budget={OPENAI_PROMPT_BUDGET}",
            req.prompt.len()
        );
    }

    #[test]
    fn truncated_prompt_carries_marker() {
        let long_prompt = "x".repeat(OPENAI_PROMPT_LIMIT + 100);
        let req = OpenAiProvider::build_generate_request(
            &long_prompt,
            1,
            ImageSize::Landscape1536x1024,
            ImageBackground::Opaque,
        );
        assert!(
            req.prompt.starts_with("[truncated "),
            "truncated prompt must carry marker"
        );
    }

    #[test]
    fn short_prompt_not_truncated() {
        let prompt = "A short prompt.";
        let req = OpenAiProvider::build_generate_request(
            prompt,
            1,
            ImageSize::Landscape1536x1024,
            ImageBackground::Opaque,
        );
        assert_eq!(req.prompt, prompt);
    }

    #[test]
    fn request_body_json_has_required_fields() {
        let req = OpenAiProvider::build_generate_request(
            "Test prompt",
            2,
            ImageSize::Square1024x1024,
            ImageBackground::Opaque,
        );
        let json = serde_json::to_string(&req).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v["model"].is_string());
        assert!(v["prompt"].is_string());
        assert!(v["n"].is_number());
        assert!(v["size"].is_string());
        assert!(v["response_format"].is_string());
    }
}
