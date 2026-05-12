//! HTTP client for the Ollama REST API.
//!
//! The server-process lifecycle is managed by [`OllamaProcess`] in
//! [`crate::setup`], re-exported here for backward compatibility.

pub use crate::setup::OllamaProcess;
