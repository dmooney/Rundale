//! Offline tile art pipeline for NLS historic map tiles.
//!
//! Pre-generates stylized map tiles from raw NLS OS Ireland raster tiles.
//! Used by `parish-geo-tool seed-tiles` to populate a `bundled_tiles_dir`
//! before shipping; the live game serves those pre-generated tiles from the
//! existing [`parish_core::TileCache`] without any runtime processing.
//!
//! Two backends:
//! - [`TileArtist::Parchment`] — deterministic CPU pipeline, always offline.
//! - [`TileArtist::Diffusion`] — local Stable Diffusion img2img; falls back
//!   to parchment automatically when the endpoint is unreachable.

pub mod config;
pub mod error;

mod diffusion;
mod parchment;

pub use config::{ArtConfig, ParchmentConfig};
pub use error::ArtError;

use diffusion::DiffusionPipeline;
use parchment::ParchmentPipeline;

/// Enum-dispatched art pipeline.
///
/// Constructed once by the seeder and used concurrently via [`std::sync::Arc`].
/// Both variants are `Send + Sync`.
pub enum TileArtist {
    Parchment(ParchmentPipeline),
    Diffusion(DiffusionPipeline),
}

impl TileArtist {
    /// Build a `TileArtist` from the seeder config.
    pub fn from_config(cfg: &ArtConfig) -> Self {
        match cfg {
            ArtConfig::Parchment(p) => Self::Parchment(ParchmentPipeline::new(p.clone())),
            ArtConfig::Diffusion {
                endpoint,
                model,
                prompt,
                denoising_strength,
                controlnet_strength,
                fallback,
            } => Self::Diffusion(DiffusionPipeline::new(
                endpoint.clone(),
                model.clone(),
                prompt.clone(),
                *denoising_strength,
                *controlnet_strength,
                fallback.clone(),
            )),
        }
    }

    /// Paint `png_bytes` into a stylized tile.
    ///
    /// `z`, `x`, `y` are the XYZ tile coordinates — used to seed the
    /// deterministic paper grain so adjacent tiles have distinct texture.
    ///
    /// For the `Parchment` variant, the CPU work runs inside
    /// `tokio::task::spawn_blocking` so the calling async task is never
    /// blocked.  For `Diffusion`, the HTTP call runs in the async context
    /// directly.
    pub async fn paint_tile(
        &self,
        png_bytes: &[u8],
        z: u32,
        x: u32,
        y: u32,
    ) -> Result<Vec<u8>, ArtError> {
        match self {
            Self::Parchment(p) => {
                let p = p.clone();
                let bytes = png_bytes.to_vec();
                tokio::task::spawn_blocking(move || p.paint_sync(&bytes, z, x, y))
                    .await
                    .map_err(|e| ArtError::Spawn(e.to_string()))?
            }
            Self::Diffusion(d) => d.paint(png_bytes, z, x, y).await,
        }
    }
}
