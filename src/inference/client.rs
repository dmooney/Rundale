//! HTTP client for the Ollama REST API at localhost:11434.

use crate::error::ParishError;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::process::{Child, Command};
use std::time::Duration;

/// HTTP client for the Ollama local inference API.
///
/// Wraps `reqwest::Client` with a configurable base URL and 30-second
/// default timeout. Provides both plain-text and structured JSON
/// completion methods.
#[derive(Clone)]
pub struct OllamaClient {
    /// The underlying HTTP client.
    client: reqwest::Client,
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
#[derive(Deserialize)]
struct GenerateResponse {
    #[serde(default)]
    response: String,
}

impl OllamaClient {
    /// Creates a new Ollama client with a 30-second request timeout.
    pub fn new(base_url: &str) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("failed to build reqwest client");

        Self {
            client,
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
    /// Checks if Ollama is reachable. If not, starts `ollama serve` in the
    /// background and waits for it to become ready (up to 30 seconds).
    ///
    /// Returns an `OllamaProcess` that will stop the server on drop if
    /// we started it.
    pub async fn ensure_running(base_url: &str) -> Result<Self, ParishError> {
        if Self::is_reachable(base_url).await {
            tracing::info!("Ollama already running at {}", base_url);
            return Ok(Self { child: None });
        }

        tracing::info!("Ollama not detected, starting ollama serve...");

        let child = Command::new("ollama")
            .arg("serve")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| {
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
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        client.get(base_url).send().await.is_ok()
    }

    /// Stops the Ollama process if we started it.
    pub fn stop(&mut self) {
        if let Some(ref mut child) = self.child {
            tracing::info!("Stopping Ollama server...");
            let _ = child.kill();
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
