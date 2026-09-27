//! HTTP client for the Ollama REST API.
//!
//! The server-process lifecycle is managed by [`OllamaProcess`] in
//! [`crate::setup`], re-exported here for backward compatibility. The
//! vllm-mlx and vllm runtimes add parallel process/slot re-exports for
//! the macOS and Linux/Windows two-slot loadouts.

pub use crate::setup::{
    OllamaProcess, RuntimeProcesses, VllmMlxProcess, VllmMlxSlot, VllmProcess, VllmSlot,
};
