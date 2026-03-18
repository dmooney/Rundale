/// Core error types for the Parish game engine.
#[derive(Debug, thiserror::Error)]
pub enum ParishError {
    #[error("inference error: {0}")]
    Inference(String),

    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
