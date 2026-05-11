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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_inference() {
        let err = ParishError::Inference("timeout".into());
        assert_eq!(err.to_string(), "inference error: timeout");
    }

    #[test]
    fn test_display_setup() {
        let err = ParishError::Setup("missing config".into());
        assert_eq!(err.to_string(), "setup error: missing config");
    }

    #[test]
    fn test_display_world_graph() {
        let err = ParishError::WorldGraph("disconnected node".into());
        assert_eq!(err.to_string(), "world graph error: disconnected node");
    }

    #[test]
    fn test_display_model_not_available() {
        let err = ParishError::ModelNotAvailable("llama3".into());
        assert_eq!(err.to_string(), "model not available: llama3");
    }

    #[test]
    fn test_display_database() {
        let err = ParishError::Database("disk full".into());
        assert_eq!(err.to_string(), "database error: disk full");
    }

    #[test]
    fn test_display_network() {
        let err = ParishError::Network("connection refused".into());
        assert_eq!(err.to_string(), "network error: connection refused");
    }

    #[test]
    fn test_display_config() {
        let err = ParishError::Config("invalid key".into());
        assert_eq!(err.to_string(), "configuration error: invalid key");
    }

    #[test]
    fn test_display_inference_json_parse_failed() {
        let err = ParishError::InferenceJsonParseFailed("expected object".into());
        assert_eq!(
            err.to_string(),
            "inference JSON parse failed: expected object"
        );
    }

    #[test]
    fn test_from_serde_json_error() {
        let invalid = r#"not json"#;
        let err: ParishError = serde_json::from_str::<serde_json::Value>(invalid)
            .unwrap_err()
            .into();
        assert!(err.to_string().starts_with("serialization error:"));
    }

    #[test]
    fn test_from_io_error() {
        use std::io;
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let err: ParishError = io_err.into();
        assert_eq!(err.to_string(), "io error: file not found");
    }

    #[test]
    fn test_variant_names() {
        assert!(matches!(
            ParishError::Inference("a".into()),
            ParishError::Inference(_)
        ));
        assert!(matches!(
            ParishError::Setup("a".into()),
            ParishError::Setup(_)
        ));
        assert!(matches!(
            ParishError::Config("a".into()),
            ParishError::Config(_)
        ));
        assert!(matches!(
            ParishError::Database("a".into()),
            ParishError::Database(_)
        ));
        assert!(matches!(
            ParishError::Network("a".into()),
            ParishError::Network(_)
        ));
        assert!(matches!(
            ParishError::WorldGraph("a".into()),
            ParishError::WorldGraph(_)
        ));
        assert!(matches!(
            ParishError::ModelNotAvailable("a".into()),
            ParishError::ModelNotAvailable(_)
        ));
        assert!(matches!(
            ParishError::InferenceJsonParseFailed("a".into()),
            ParishError::InferenceJsonParseFailed(_)
        ));
    }
}
