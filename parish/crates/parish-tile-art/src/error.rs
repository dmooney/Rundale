use thiserror::Error;

#[derive(Debug, Error)]
pub enum ArtError {
    #[error("image codec: {0}")]
    Image(String),
    #[error("diffusion endpoint: {0}")]
    Diffusion(String),
    #[error("spawn_blocking join: {0}")]
    Spawn(String),
}

impl From<image::ImageError> for ArtError {
    fn from(e: image::ImageError) -> Self {
        Self::Image(e.to_string())
    }
}
