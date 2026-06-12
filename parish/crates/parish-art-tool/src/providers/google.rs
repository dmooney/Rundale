//! Google Imagen 3 provider via the Gemini API (Vertex AI / generativelanguage).
//!
//! API key from `GEMINI_API_KEY` environment variable.
//!
//! # Prompt size limit (AGENTS rule 16)
//!
//! Imagen 3 via the Gemini API documents a per-instance prompt limit of
//! **480 characters** (per the Imagen 3 documentation as of 2025-06).
//! We enforce 90% of that (432 chars) client-side and truncate with a
//! `[truncated N chars]` marker.
//!
//! NOTE: The request-body shape follows the Gemini `imagen-3.0-generate-002`
//! predict endpoint:
//!   POST https://generativelanguage.googleapis.com/v1beta/models/<model>:predict
//!
//! with a JSON body of:
//!   { "instances": [{ "prompt": "..." }], "parameters": { ... } }
//!
//! This matches the documented schema for the Gemini Imagen 3 model.
//! If the endpoint or schema changes, update the body structs here so the
//! discrepancy is visible at review time.

use std::path::Path;

use anyhow::{Result, bail};
use base64::Engine;
use serde::{Deserialize, Serialize};

use super::{ImageBackground, ImageProvider, ImageSize, apply_prompt_budget};

// ---------------------------------------------------------------------------
// Prompt-size budget (AGENTS rule 16)
// ---------------------------------------------------------------------------

/// Imagen 3 documented maximum prompt length (characters per instance).
///
/// Source: Google Imagen 3 documentation for the Gemini API,
/// `imagen-3.0-generate-002` model.
/// NOTE: If the provider raises this limit, update the constant and the test.
pub const GOOGLE_PROMPT_LIMIT: usize = 480;

/// Client-side budget = 90% of the provider limit.
/// A `payload.len() <= GOOGLE_PROMPT_BUDGET` assertion lives in the tests.
pub const GOOGLE_PROMPT_BUDGET: usize = (GOOGLE_PROMPT_LIMIT as f64 * 0.90) as usize;

// ---------------------------------------------------------------------------
// Request-body types
// ---------------------------------------------------------------------------

/// One prompt instance in the Imagen 3 predict request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GoogleInstance {
    pub prompt: String,
}

/// Parameters block in the Imagen 3 predict request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GoogleParameters {
    /// Number of images to generate.
    pub sample_count: u32,
    /// Aspect ratio string (e.g. "16:9" for plates, "1:1" for sprites).
    pub aspect_ratio: String,
    /// Optional person generation mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub person_generation: Option<String>,
}

/// Full request body for the Imagen 3 predict endpoint.
///
/// POST .../models/imagen-3.0-generate-002:predict
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GoogleGenerateRequest {
    pub instances: Vec<GoogleInstance>,
    pub parameters: GoogleParameters,
}

/// Response from the Imagen 3 predict endpoint.
#[derive(Debug, Deserialize)]
pub struct GoogleGenerateResponse {
    pub predictions: Vec<GooglePrediction>,
}

/// One prediction (generated image).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GooglePrediction {
    /// Base64-encoded PNG bytes.
    pub bytes_base64_encoded: Option<String>,
    /// MIME type of the image.
    pub mime_type: Option<String>,
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// The Google/Gemini API key from the environment: `GEMINI_API_KEY` first, then
/// `GOOGLE_API_KEY`. An empty value counts as unset. `None` when neither has a
/// non-empty value.
pub fn google_api_key() -> Option<String> {
    for var in ["GEMINI_API_KEY", "GOOGLE_API_KEY"] {
        if let Ok(v) = std::env::var(var) {
            if !v.trim().is_empty() {
                return Some(v);
            }
        }
    }
    None
}

/// Google Imagen 3 provider.
pub struct GoogleProvider {
    api_key: String,
    base_url: String,
    model: String,
}

impl GoogleProvider {
    /// Create a new provider, reading the key from `GEMINI_API_KEY`, falling
    /// back to `GOOGLE_API_KEY`. Both names are in common use — the Gemini API
    /// docs favour `GEMINI_API_KEY` while the broader Google SDK convention is
    /// `GOOGLE_API_KEY` — so either populated variable works.
    pub fn from_env() -> Result<Self> {
        let api_key = google_api_key()
            .ok_or_else(|| anyhow::anyhow!("neither GEMINI_API_KEY nor GOOGLE_API_KEY is set"))?;
        Ok(Self {
            api_key,
            base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
            model: "imagen-3.0-generate-002".to_string(),
        })
    }

    /// Build a generate request body (enforces prompt budget per rule 16).
    ///
    /// `aspect_ratio`: "16:9" for landscape plates, "1:1" for square sprites.
    pub fn build_generate_request(prompt: &str, n: u32, size: ImageSize) -> GoogleGenerateRequest {
        let safe_prompt = apply_prompt_budget(prompt, GOOGLE_PROMPT_BUDGET);
        let aspect_ratio = match size {
            ImageSize::Landscape1536x1024 => "16:9",
            ImageSize::Square1024x1024 => "1:1",
        };
        GoogleGenerateRequest {
            instances: vec![GoogleInstance {
                prompt: safe_prompt,
            }],
            parameters: GoogleParameters {
                sample_count: n,
                aspect_ratio: aspect_ratio.to_string(),
                person_generation: Some("allow_adult".to_string()),
            },
        }
    }
}

impl ImageProvider for GoogleProvider {
    fn generate(
        &self,
        prompt: &str,
        n: u32,
        size: ImageSize,
        _background: ImageBackground,
    ) -> Result<Vec<Vec<u8>>> {
        let request = Self::build_generate_request(prompt, n, size);
        let body = serde_json::to_string(&request)?;

        let url = format!(
            "{}/models/{}:predict?key={}",
            self.base_url, self.model, self.api_key
        );
        let client = reqwest::blocking::Client::new();
        let resp = client
            .post(&url)
            .header("Content-Type", "application/json")
            .body(body)
            .send()?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            bail!("Google Imagen API error {status}: {text}");
        }

        let response: GoogleGenerateResponse = resp.json()?;
        let mut images = Vec::new();
        for pred in response.predictions {
            if let Some(b64) = pred.bytes_base64_encoded {
                let bytes = base64::engine::general_purpose::STANDARD.decode(&b64)?;
                images.push(bytes);
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
        // Imagen 3 via the Gemini API does not expose an img2img edit endpoint
        // in the same way as OpenAI.  Style consistency is achieved by passing
        // anchor images as separate prompt instances and using the reference in
        // the text prompt.  This is a known limitation documented here.
        bail!("Imagen 3 edit endpoint not yet implemented — use gen with anchor prompt")
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
        let req = GoogleProvider::build_generate_request(
            "A pixel art plate of Darcy's Pub.",
            3,
            ImageSize::Landscape1536x1024,
        );
        let json = serde_json::to_string(&req).unwrap();
        let roundtripped: GoogleGenerateRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, roundtripped);
    }

    #[test]
    fn plate_request_has_16_9_aspect_ratio() {
        let req = GoogleProvider::build_generate_request(
            "A pixel art plate.",
            1,
            ImageSize::Landscape1536x1024,
        );
        assert_eq!(req.parameters.aspect_ratio, "16:9");
    }

    #[test]
    fn sprite_request_has_1_1_aspect_ratio() {
        let req = GoogleProvider::build_generate_request(
            "A pixel art sprite.",
            1,
            ImageSize::Square1024x1024,
        );
        assert_eq!(req.parameters.aspect_ratio, "1:1");
    }

    /// AGENTS rule 16: prompt field length <= GOOGLE_PROMPT_BUDGET.
    #[test]
    fn prompt_field_len_within_budget() {
        let long_prompt = "x".repeat(GOOGLE_PROMPT_LIMIT + 200);
        let req =
            GoogleProvider::build_generate_request(&long_prompt, 1, ImageSize::Square1024x1024);
        let prompt_len = req.instances[0].prompt.len();
        assert!(
            prompt_len <= GOOGLE_PROMPT_BUDGET,
            "prompt.len()={prompt_len} > budget={GOOGLE_PROMPT_BUDGET}"
        );
    }

    #[test]
    fn truncated_prompt_carries_marker() {
        let long_prompt = "y".repeat(GOOGLE_PROMPT_LIMIT + 50);
        let req =
            GoogleProvider::build_generate_request(&long_prompt, 1, ImageSize::Square1024x1024);
        assert!(
            req.instances[0].prompt.starts_with("[truncated "),
            "truncated prompt must carry marker"
        );
    }

    #[test]
    fn short_prompt_not_truncated() {
        let prompt = "A short Irish scene.";
        let req = GoogleProvider::build_generate_request(prompt, 1, ImageSize::Landscape1536x1024);
        assert_eq!(req.instances[0].prompt, prompt);
    }

    #[test]
    fn request_body_json_has_required_fields() {
        let req = GoogleProvider::build_generate_request("Test", 2, ImageSize::Square1024x1024);
        let json = serde_json::to_string(&req).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v["instances"].is_array());
        assert!(v["instances"][0]["prompt"].is_string());
        assert!(v["parameters"]["sampleCount"].is_number());
        assert!(v["parameters"]["aspectRatio"].is_string());
    }
}
