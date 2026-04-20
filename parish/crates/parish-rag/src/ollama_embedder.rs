//! Live embedder backed by Ollama's `/api/embeddings` endpoint.
//!
//! Sends `{"model": "...", "prompt": "..."}` and parses `{"embedding": [...]}`
//! — the shape Ollama has exposed since 0.1.x. OpenAI's `/v1/embeddings` is a
//! different schema; the demo targets Ollama because the rest of the project
//! already does.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Ollama embeddings client.
#[derive(Debug, Clone)]
pub struct OllamaEmbedder {
    client: reqwest::Client,
    base_url: String,
    model: String,
}

#[derive(Serialize)]
struct EmbedRequest<'a> {
    model: &'a str,
    prompt: &'a str,
}

#[derive(Deserialize)]
struct EmbedResponse {
    #[serde(default)]
    embedding: Vec<f32>,
}

impl OllamaEmbedder {
    /// Creates a new client pointing at an Ollama server.
    ///
    /// `base_url` defaults to `http://localhost:11434` when empty.
    /// `model` is a registered embedding model (e.g. `nomic-embed-text`).
    pub fn new(base_url: &str, model: &str) -> Self {
        let base_url = if base_url.trim().is_empty() {
            "http://localhost:11434".to_string()
        } else {
            base_url.trim_end_matches('/').to_string()
        };
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            client,
            base_url,
            model: model.to_string(),
        }
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Embed `text` via Ollama. Returns the error message verbatim on failure
    /// so CLI callers can surface it without extra wrapping.
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        let url = format!("{}/api/embeddings", self.base_url);
        let body = EmbedRequest {
            model: &self.model,
            prompt: text,
        };
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("ollama embed request failed: {e}"))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("ollama embed HTTP {status}: {text}"));
        }
        let parsed: EmbedResponse = resp
            .json()
            .await
            .map_err(|e| format!("ollama embed JSON parse failed: {e}"))?;
        if parsed.embedding.is_empty() {
            return Err("ollama returned an empty embedding".to_string());
        }
        Ok(parsed.embedding)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_defaults_when_empty() {
        let e = OllamaEmbedder::new("", "nomic-embed-text");
        assert_eq!(e.base_url(), "http://localhost:11434");
    }

    #[test]
    fn base_url_strips_trailing_slash() {
        let e = OllamaEmbedder::new("http://localhost:11434/", "m");
        assert_eq!(e.base_url(), "http://localhost:11434");
    }

    #[test]
    fn model_is_recorded() {
        let e = OllamaEmbedder::new("http://localhost:11434", "nomic-embed-text");
        assert_eq!(e.model(), "nomic-embed-text");
    }

    /// Request body shape must match Ollama's `/api/embeddings`: `{model, prompt}`.
    /// A regression here would silently return 404 from any real server.
    #[test]
    fn request_body_serialises_to_ollama_schema() {
        let body = EmbedRequest {
            model: "nomic-embed-text",
            prompt: "hello",
        };
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["model"], "nomic-embed-text");
        assert_eq!(json["prompt"], "hello");
    }

    #[test]
    fn response_deserialises_embedding() {
        let json = r#"{"embedding": [0.1, 0.2, 0.3]}"#;
        let parsed: EmbedResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.embedding, vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn response_missing_embedding_is_empty() {
        let json = r#"{}"#;
        let parsed: EmbedResponse = serde_json::from_str(json).unwrap();
        assert!(parsed.embedding.is_empty());
    }
}
