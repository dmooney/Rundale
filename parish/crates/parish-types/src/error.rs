/// Core error types for the Parish game engine.
#[derive(Debug, thiserror::Error)]
pub enum ParishError {
    #[error("inference error: {0}")]
    Inference(String),

    #[error("setup error: {0}")]
    Setup(String),

    #[error("world graph error: {0}")]
    WorldGraph(String),

    #[error("model not available: {0}")]
    ModelNotAvailable(String),

    /// Database error. Stored as a string so that `parish-types` does not
    /// need to depend on `rusqlite`. Higher-level crates (e.g.
    /// `parish-persistence`) convert `rusqlite::Error` via `.to_string()`
    /// before wrapping here. (#699)
    #[error("database error: {0}")]
    Database(String),

    /// Network error. Stored as a string so that `parish-types` does not
    /// need to depend on `reqwest`. Higher-level crates convert
    /// `reqwest::Error` via `.to_string()` before wrapping here. (#699)
    #[error("network error: {0}")]
    Network(String),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("configuration error: {0}")]
    Config(String),

    /// Inference returned a response that could not be parsed as the expected
    /// JSON schema, even after a retry. Distinct from [`ParishError::Inference`]
    /// (transport / HTTP error) so callers can distinguish a schema mismatch
    /// from a provider connectivity failure. (#416)
    #[error("inference JSON parse failed: {0}")]
    InferenceJsonParseFailed(String),
}
