//! HTTP client for the Ollama REST API at localhost:11434.

use crate::TOKEN_CHANNEL_CAPACITY;
use parish_config::InferenceConfig;
use parish_types::ParishError;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::process::{Child, Command};
use std::time::Duration;
use tokio::sync::mpsc;

/// HTTP client for the Ollama local inference API.
///
/// Wraps `reqwest::Client` with a configurable base URL and 30-second
/// default timeout. Provides both plain-text and structured JSON
/// completion methods.
#[derive(Clone)]
pub struct OllamaClient {
    /// HTTP client with default timeout for non-streaming requests.
    client: reqwest::Client,
    /// HTTP client with longer timeout for streaming requests.
    /// Reused across calls to preserve connection pooling.
    streaming_client: reqwest::Client,
    /// Base URL for the Ollama API (e.g. "http://localhost:11434").
    base_url: String,
}

/// Request body for the Ollama `/api/generate` endpoint.
#[derive(Serialize)]
struct GenerateRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<&'a str>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<&'a str>,
}

/// Response body from the Ollama `/api/generate` endpoint.
///
/// Used for both non-streaming (single JSON object) and streaming
/// (NDJSON, one per token) responses.
#[derive(Deserialize, Debug)]
pub(crate) struct GenerateResponse {
    /// The generated text (full response or single token).
    #[serde(default)]
    pub(crate) response: String,
    /// Whether this is the final chunk in a streaming response.
    #[serde(default)]
    pub(crate) done: bool,
}

impl OllamaClient {
    /// Creates a new Ollama client with default timeouts (30s request, 300s streaming).
    pub fn new(base_url: &str) -> Self {
        Self::new_with_config(base_url, &InferenceConfig::default())
    }

    /// Creates a new Ollama client with timeouts sourced from `InferenceConfig`.
    ///
    /// Uses `config.timeout_secs` for the default HTTP client and stores
    /// `config.streaming_timeout_secs` for streaming request clients.
    ///
    /// If the underlying `reqwest` builder fails (e.g. a TLS backend is
    /// unavailable), this falls back to a default `reqwest::Client` with
    /// no configured timeout rather than panicking, and emits a warning
    /// via `tracing`. See issue #98.
    pub fn new_with_config(base_url: &str, config: &InferenceConfig) -> Self {
        let client = crate::openai_client::build_client_or_fallback(
            Duration::from_secs(config.timeout_secs),
            "Ollama",
        );

        // Pre-build the streaming client once so connection pooling is
        // preserved across streaming calls instead of creating a fresh
        // client (and fresh TCP connections) on every request.
        let streaming_client = crate::openai_client::build_client_or_fallback(
            Duration::from_secs(config.streaming_timeout_secs),
            "Ollama streaming",
        );

        Self {
            client,
            streaming_client,
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// Sends a completion request and returns the full response text.
    ///
    /// Calls POST `/api/generate` with `stream: false` and waits for
    /// the complete response.
    pub async fn generate(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
    ) -> Result<String, ParishError> {
        let url = format!("{}/api/generate", self.base_url);
        let body = GenerateRequest {
            model,
            prompt,
            system,
            stream: false,
            format: None,
        };

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await?
            .error_for_status()
            .map_err(|e| ParishError::Inference(e.to_string()))?;

        let gen_resp: GenerateResponse = resp.json().await?;
        Ok(gen_resp.response)
    }

    /// Sends a streaming completion request, forwarding tokens as they arrive.
    ///
    /// Calls POST `/api/generate` with `stream: true`. Each token is sent
    /// through `token_tx` as it arrives. Returns the full accumulated text
    /// after the stream completes. Uses a 5-minute timeout to accommodate
    /// long generations from large models.
    pub async fn generate_stream(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        token_tx: mpsc::Sender<String>,
    ) -> Result<String, ParishError> {
        let url = format!("{}/api/generate", self.base_url);
        let body = GenerateRequest {
            model,
            prompt,
            system,
            stream: true,
            format: None,
        };

        let resp = self
            .streaming_client
            .post(&url)
            .json(&body)
            .send()
            .await?
            .error_for_status()
            .map_err(|e| ParishError::Inference(e.to_string()))?;

        let mut accumulated = String::new();
        let mut line_buf = String::new();
        let mut decoder = crate::utf8_stream::Utf8StreamDecoder::new();

        // Read chunks and split into NDJSON lines
        let mut response = resp;
        while let Some(chunk) = response.chunk().await? {
            // Decode incrementally so multi-byte characters split across
            // HTTP chunk boundaries aren't mangled into U+FFFD (#223).
            line_buf.push_str(&decoder.push(&chunk));

            // Process complete lines
            while let Some(newline_pos) = line_buf.find('\n') {
                let line: String = line_buf.drain(..=newline_pos).collect();
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                if let Ok(gen_resp) = serde_json::from_str::<GenerateResponse>(line) {
                    if !gen_resp.response.is_empty() {
                        if token_tx.try_send(gen_resp.response.clone()).is_err() {
                            tracing::warn!(
                                "token streaming channel full (capacity {}); token dropped — \
                                 consumer is not keeping up with LLM output (#83)",
                                TOKEN_CHANNEL_CAPACITY,
                            );
                        }
                        accumulated.push_str(&gen_resp.response);
                    }
                    if gen_resp.done {
                        return Ok(accumulated);
                    }
                }
            }
        }

        // Flush any trailing incomplete bytes, then process any remaining line.
        line_buf.push_str(&decoder.flush());
        let remaining = line_buf.trim();
        if !remaining.is_empty()
            && let Ok(gen_resp) = serde_json::from_str::<GenerateResponse>(remaining)
            && !gen_resp.response.is_empty()
        {
            if token_tx.try_send(gen_resp.response.clone()).is_err() {
                tracing::warn!(
                    "token streaming channel full (capacity {}); token dropped — \
                     consumer is not keeping up with LLM output (#83)",
                    TOKEN_CHANNEL_CAPACITY,
                );
            }
            accumulated.push_str(&gen_resp.response);
        }

        Ok(accumulated)
    }

    /// Sends a completion request and deserializes the response as structured JSON.
    ///
    /// Requests JSON format from Ollama and parses the response text
    /// into the target type `T`. Uses `#[serde(default)]` on optional
    /// fields in `T` for robustness against partial LLM output.
    pub async fn generate_json<T: DeserializeOwned>(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
    ) -> Result<T, ParishError> {
        let url = format!("{}/api/generate", self.base_url);
        let body = GenerateRequest {
            model,
            prompt,
            system,
            stream: false,
            format: Some("json"),
        };

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await?
            .error_for_status()
            .map_err(|e| ParishError::Inference(e.to_string()))?;

        let gen_resp: GenerateResponse = resp.json().await?;
        let parsed: T = serde_json::from_str(&gen_resp.response)?;
        Ok(parsed)
    }

    /// Returns the base URL of this client.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

/// Manages an Ollama server process started by Parish.
///
/// If Ollama was not already running when the game started, this struct
/// holds the child process handle. When dropped, it kills the process
/// to clean up. If Ollama was already running, this is a no-op wrapper.
pub struct OllamaProcess {
    child: Option<Child>,
}

impl OllamaProcess {
    /// Creates a no-op process handle (for non-Ollama providers).
    pub fn none() -> Self {
        Self { child: None }
    }

    /// Checks if Ollama is reachable. If not, starts `ollama serve` in the
    /// background and waits for it to become ready (up to 30 seconds).
    ///
    /// The optional `gpu_env` parameter allows injecting environment variables
    /// into the spawned process (e.g. `OLLAMA_VULKAN=1` for AMD GPUs on Windows).
    /// These are only applied when Parish starts Ollama itself; if Ollama is
    /// already running, the caller should restart it manually to change env vars.
    ///
    /// Returns an `OllamaProcess` that will stop the server on drop if
    /// we started it.
    pub async fn ensure_running(
        base_url: &str,
        gpu_env: Option<&[(String, String)]>,
    ) -> Result<Self, ParishError> {
        if Self::is_reachable(base_url).await {
            tracing::info!("Ollama already running at {}", base_url);
            return Ok(Self { child: None });
        }

        tracing::info!("Ollama not detected, starting ollama serve...");

        let mut cmd = Command::new("ollama");
        cmd.arg("serve")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());

        if let Some(env_vars) = gpu_env {
            for (key, value) in env_vars {
                cmd.env(key, value);
            }
        }

        let child = cmd.spawn().map_err(|e| {
            ParishError::Inference(format!(
                "failed to start ollama serve: {}. Is ollama installed?",
                e
            ))
        })?;

        // Wait for Ollama to become reachable
        let mut ready = false;
        for i in 0..60 {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if Self::is_reachable(base_url).await {
                tracing::info!("Ollama ready after ~{}ms", (i + 1) * 500);
                ready = true;
                break;
            }
        }

        if !ready {
            return Err(ParishError::Inference(
                "ollama serve started but did not become reachable within 30s".to_string(),
            ));
        }

        Ok(Self { child: Some(child) })
    }

    /// Returns whether we started the Ollama process (vs. it was already running).
    pub fn was_started_by_us(&self) -> bool {
        self.child.is_some()
    }

    /// Checks if the Ollama API is reachable by hitting the root endpoint.
    async fn is_reachable(base_url: &str) -> bool {
        // Use the shared builder helper so a failing reqwest build falls
        // back to a default client instead of panicking (#98).
        let client = crate::openai_client::build_client_or_fallback(
            Duration::from_secs(2),
            "Ollama reachability probe",
        );
        client.get(base_url).send().await.is_ok()
    }

    /// Stops the Ollama process if we started it.
    ///
    /// On Windows, uses `taskkill /F /T /PID` to kill the entire process
    /// tree, ensuring GPU worker processes are also terminated and VRAM
    /// is released. On other platforms, uses the standard `kill()`.
    pub fn stop(&mut self) {
        if let Some(ref mut child) = self.child {
            tracing::info!("Stopping Ollama server...");

            #[cfg(target_os = "windows")]
            {
                let pid = child.id();
                // Kill the entire process tree so GPU workers release VRAM
                let _ = Command::new("taskkill")
                    .args(["/F", "/T", "/PID", &pid.to_string()])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }

            #[cfg(not(target_os = "windows"))]
            {
                let _ = child.kill();
            }

            let _ = child.wait();
            self.child = None;
        }
    }
}

impl Drop for OllamaProcess {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_client_new() {
        let client = OllamaClient::new("http://localhost:11434");
        assert_eq!(client.base_url(), "http://localhost:11434");
    }

    #[test]
    fn test_ollama_client_trailing_slash() {
        let client = OllamaClient::new("http://localhost:11434/");
        assert_eq!(client.base_url(), "http://localhost:11434");
    }

    #[test]
    fn test_generate_response_deserialize() {
        let json = r#"{"response": "Hello, world!"}"#;
        let resp: GenerateResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.response, "Hello, world!");
    }

    #[test]
    fn test_generate_response_missing_field() {
        let json = r#"{}"#;
        let resp: GenerateResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.response, "");
        assert!(!resp.done);
    }

    #[test]
    fn test_generate_response_streaming_chunk() {
        let json = r#"{"model":"qwen3:14b","response":"Hello","done":false}"#;
        let resp: GenerateResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.response, "Hello");
        assert!(!resp.done);
    }

    #[test]
    fn test_generate_response_streaming_final() {
        let json = r#"{"model":"qwen3:14b","response":"","done":true}"#;
        let resp: GenerateResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.response, "");
        assert!(resp.done);
    }

    #[tokio::test]
    #[ignore] // Requires Ollama running on localhost:11434
    async fn test_generate_live() {
        let client = OllamaClient::new("http://localhost:11434");
        let result = client
            .generate("qwen3:14b", "Say hello in one word.", None)
            .await;
        assert!(result.is_ok());
        assert!(!result.unwrap().is_empty());
    }
}
