//! LLM inference pipeline via Ollama.
//!
//! Manages a request queue (Tokio mpsc channel), routes requests
//! to the appropriate model based on cognitive LOD tier, and
//! parses structured JSON responses.

pub mod client;

// TODO: InferenceRequest / InferenceResponse types
// TODO: Inference queue (tokio mpsc channel)
// TODO: Inference worker task
// TODO: Tiered model selection (14B, 8B, 3B)
