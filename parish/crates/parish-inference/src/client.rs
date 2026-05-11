//! HTTP client for the Ollama REST API.
//!
//! The server-process lifecycle is managed by [`OllamaProcess`] in
//! [`crate::setup`], re-exported here for backward compatibility. The
//! vllm-mlx runtime adds parallel `VllmMlxProcess` / `VllmMlxSlot` /
//! `RuntimeProcesses` re-exports for the macOS two-slot loadout.

pub use crate::setup::{OllamaProcess, RuntimeProcesses, VllmMlxProcess, VllmMlxSlot};
