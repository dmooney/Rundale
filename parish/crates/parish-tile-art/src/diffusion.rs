//! AI img2img tile stylization via a local Stable Diffusion WebUI.
//!
//! Sends the raw NLS tile to a local A1111-compatible `/sdapi/v1/img2img`
//! endpoint.  A ControlNet Canny pre-pass is included in the request to keep
//! geographic features recognisable while allowing strong painterly stylization.
//!
//! Falls back to [`crate::parchment::ParchmentPipeline`] automatically if the
//! endpoint is unreachable or returns an error.

use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use image::ImageFormat;
use serde::Deserialize;
use serde_json::json;

use crate::{config::ParchmentConfig, error::ArtError, parchment::ParchmentPipeline};

pub struct DiffusionPipeline {
    endpoint: String,
    model: Option<String>,
    prompt: String,
    denoising_strength: f32,
    controlnet_strength: f32,
    fallback: ParchmentPipeline,
    http: reqwest::Client,
}

impl DiffusionPipeline {
    pub fn new(
        endpoint: String,
        model: Option<String>,
        prompt: String,
        denoising_strength: f32,
        controlnet_strength: f32,
        fallback_cfg: ParchmentConfig,
    ) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .user_agent("Parish-TileArt/0.1")
            .build()
            .expect("reqwest client build");
        Self {
            endpoint,
            model,
            prompt,
            denoising_strength,
            controlnet_strength,
            fallback: ParchmentPipeline::new(fallback_cfg),
            http,
        }
    }

    pub async fn paint(
        &self,
        png_bytes: &[u8],
        z: u32,
        x: u32,
        y: u32,
    ) -> Result<Vec<u8>, ArtError> {
        match self.try_diffusion(png_bytes).await {
            Ok(result) => Ok(result),
            Err(e) => {
                tracing::warn!(
                    z, x, y,
                    error = %e,
                    "diffusion endpoint failed — falling back to parchment pipeline"
                );
                self.fallback.paint_sync(png_bytes, z, x, y)
            }
        }
    }

    async fn try_diffusion(&self, png_bytes: &[u8]) -> Result<Vec<u8>, ArtError> {
        let raw_b64 = format!("data:image/png;base64,{}", B64.encode(png_bytes));

        // Pre-process: Canny edges as ControlNet hint
        let canny_b64 = {
            let img = image::load_from_memory(png_bytes)?.to_luma8();
            let edges = imageproc::edges::canny(&img, 50.0, 150.0);
            let mut canny_png = Vec::new();
            image::DynamicImage::ImageLuma8(edges)
                .write_to(&mut std::io::Cursor::new(&mut canny_png), ImageFormat::Png)?;
            format!("data:image/png;base64,{}", B64.encode(&canny_png))
        };

        let mut body = json!({
            "prompt": self.prompt,
            "negative_prompt": "modern, photograph, satellite, aerial, digital, contemporary, blurry",
            "init_images": [raw_b64],
            "denoising_strength": self.denoising_strength,
            "width": 256,
            "height": 256,
            "cfg_scale": 7,
            "sampler_name": "DPM++ 2M Karras",
            "steps": 20,
            "alwayson_scripts": {
                "ControlNet": {
                    "args": [{
                        "enabled": true,
                        "image": canny_b64,
                        "module": "canny",
                        "model": "control_v11p_sd15_canny",
                        "weight": self.controlnet_strength,
                        "guidance_start": 0.0,
                        "guidance_end": 1.0,
                        "control_mode": 0,
                        "resize_mode": 1
                    }]
                }
            }
        });

        if let Some(ref model) = self.model {
            body["override_settings"] = json!({ "sd_model_checkpoint": model });
        }

        let url = format!("{}/sdapi/v1/img2img", self.endpoint.trim_end_matches('/'));
        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ArtError::Diffusion(format!("request: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ArtError::Diffusion(format!("HTTP {status}: {text}")));
        }

        let payload: SdResponse = resp
            .json()
            .await
            .map_err(|e| ArtError::Diffusion(format!("decode response: {e}")))?;

        let img_b64 = payload
            .images
            .into_iter()
            .next()
            .ok_or_else(|| ArtError::Diffusion("empty images array".to_string()))?;

        // Strip optional data URI prefix
        let b64_data = img_b64
            .strip_prefix("data:image/png;base64,")
            .unwrap_or(&img_b64);

        let img_bytes = B64
            .decode(b64_data)
            .map_err(|e| ArtError::Diffusion(format!("base64 decode: {e}")))?;

        // Validate it's actually a 256×256 PNG before returning
        let decoded = image::load_from_memory(&img_bytes)?;
        if decoded.width() != 256 || decoded.height() != 256 {
            // Re-encode after resize in case SD changed dimensions
            let resized = decoded.resize_exact(256, 256, image::imageops::FilterType::Lanczos3);
            let mut buf = Vec::new();
            resized.write_to(&mut std::io::Cursor::new(&mut buf), ImageFormat::Png)?;
            return Ok(buf);
        }

        Ok(img_bytes)
    }
}

#[derive(Deserialize)]
struct SdResponse {
    images: Vec<String>,
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use image::RgbImage;

    use super::*;

    fn solid_png(r: u8, g: u8, b: u8) -> Vec<u8> {
        let img = RgbImage::from_fn(256, 256, |_, _| image::Rgb([r, g, b]));
        let mut buf = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut std::io::Cursor::new(&mut buf), ImageFormat::Png)
            .unwrap();
        buf
    }

    #[tokio::test]
    async fn unreachable_endpoint_falls_back_to_parchment() {
        let pipeline = DiffusionPipeline::new(
            "http://127.0.0.1:1".to_string(), // nothing listening here
            None,
            "test prompt".to_string(),
            0.65,
            0.85,
            ParchmentConfig::default(),
        );
        let input = solid_png(200, 190, 160);
        // Should succeed via parchment fallback, not error
        let result = pipeline.paint(&input, 10, 100, 200).await;
        assert!(
            result.is_ok(),
            "fallback must not propagate error: {result:?}"
        );
    }
}
