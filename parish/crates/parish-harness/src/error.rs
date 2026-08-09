//! Error type for the harness. One enum, mapped from transport, DB, judge, and
//! rendering failures so the run loop can decide whether a failure is a *gate*
//! (a real game defect — recorded against the run) or an *operational* error
//! (the harness itself broke — surfaced to the operator).

use thiserror::Error;

/// Errors produced anywhere in the harness.
#[derive(Debug, Error)]
pub enum HarnessError {
    /// The backend could not be reached or returned a transport-level failure.
    /// Distinct from a gate: this is "no game to talk to", surfaced to the
    /// operator. (A *crash mid-run* is detected in the loop and recorded as a
    /// gate instead.)
    #[error("transport error talking to {url}: {source}")]
    Transport {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    /// The backend answered with a non-success HTTP status.
    #[error("backend rejected {method} {url} -> {status}: {body}")]
    HttpStatus {
        method: String,
        url: String,
        status: u16,
        body: String,
    },

    /// A response body could not be parsed into the expected wire type.
    #[error("invalid JSON from {context}: {source}")]
    Json {
        context: String,
        #[source]
        source: serde_json::Error,
    },

    /// The loaded rubric's content hash does not match the pinned hash.
    #[error("rubric drift: config pinned {expected} but rubric file hashes to {actual}")]
    RubricDrift { expected: String, actual: String },

    /// The judge returned a verdict that failed validation (missing axis,
    /// out-of-range score, etc.).
    #[error("judge verdict invalid: {0}")]
    Judge(String),

    /// A produced artifact (state-frame) was blank or degenerate — rule #14:
    /// never report success for empty content.
    #[error("artifact validation failed: {0}")]
    BlankArtifact(String),

    /// A rendering (PNG encode) failure.
    #[error("render error: {0}")]
    Render(String),

    /// A persistence (SQLite) failure.
    #[error("persistence error: {0}")]
    Db(#[from] rusqlite::Error),

    /// A filesystem failure writing artifacts.
    #[error("io error at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// Inference (LLM) call failure from the shared `parish-inference` client.
    #[error("inference error: {0}")]
    Inference(String),

    /// A configuration value was missing or malformed.
    #[error("config error: {0}")]
    Config(String),

    /// A completed harness invocation tripped a deterministic game-quality
    /// gate. The run is already persisted; this variant exists so automation
    /// receives a nonzero process exit instead of a misleading green result.
    #[error("run {run_id} gated: {reason}")]
    RunGated { run_id: i64, reason: String },
}

/// Convenience result alias.
pub type Result<T> = std::result::Result<T, HarnessError>;

impl HarnessError {
    /// Wrap an `std::io::Error` with the path that produced it.
    pub fn io(path: impl Into<String>, source: std::io::Error) -> Self {
        HarnessError::Io {
            path: path.into(),
            source,
        }
    }
}
