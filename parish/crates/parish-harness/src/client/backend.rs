//! HTTP game client — drives a running Parish backend over `/api/*`.
//!
//! Mirrors the dispatch shape proven in `parish-mcp`'s `ParishHttpBackend`
//! (kebab paths, GET vs POST), but exposes a typed, harness-specific surface
//! instead of a generic `invoke`. The harness never links `parish-server` /
//! `parish-tauri`; it speaks HTTP to whatever backend is listening (headless
//! server in CI, the live Tauri window when a display + vllm-mlx are present).

use async_trait::async_trait;
use serde_json::json;

use parish_core::ipc::EngineState;

use crate::config::EngineModel;
use crate::error::{HarnessError, Result};

use super::wire::CommandResponse;

/// The operations the run loop needs from a Parish backend.
#[async_trait]
pub trait GameClient: Send + Sync {
    /// Base URL the client is pointed at (for diagnostics).
    fn base_url(&self) -> &str;
    /// Liveness probe — `GET /api/health`.
    async fn health(&self) -> Result<()>;
    /// Start a fresh game — `POST /api/new-game`.
    async fn new_game(&self) -> Result<()>;
    /// Submit one player input — `POST /api/command`.
    async fn submit_command(
        &self,
        text: &str,
        addressed_to: &[String],
        timeout_ms: u64,
    ) -> Result<CommandResponse>;
    /// Read the canonical engine state — `GET /api/engine-state`.
    async fn engine_state(&self) -> Result<EngineState>;
    /// Apply a feature flag via the `/flag` slash command.
    async fn apply_flag(&self, name: &str, on: bool) -> Result<()>;
    /// Apply a per-category BYOK model override. Best-effort: a backend without
    /// the endpoint (e.g. the headless server) returns
    /// [`HarnessError::HttpStatus`] with 404, which the caller may downgrade.
    async fn apply_byok(&self, category: &str, model: &EngineModel) -> Result<()>;
}

/// Concrete HTTP client.
pub struct HttpGameClient {
    base_url: String,
    client: reqwest::Client,
}

impl HttpGameClient {
    /// Construct a client pointed at an Axum/Tauri backend root
    /// (e.g. `http://127.0.0.1:3030`).
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// GET a path and deserialize the body into `T`.
    async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = self.url(path);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|source| HarnessError::Transport {
                url: url.clone(),
                source,
            })?;
        let status = resp.status();
        let body = resp.text().await.map_err(|source| HarnessError::Transport {
            url: url.clone(),
            source,
        })?;
        if !status.is_success() {
            return Err(HarnessError::HttpStatus {
                method: "GET".into(),
                url,
                status: status.as_u16(),
                body,
            });
        }
        serde_json::from_str(&body).map_err(|source| HarnessError::Json {
            context: format!("GET {url}"),
            source,
        })
    }

    /// POST a JSON body and return the raw text (caller parses).
    async fn post_text(&self, path: &str, body: &serde_json::Value) -> Result<String> {
        let url = self.url(path);
        let resp = self
            .client
            .post(&url)
            .json(body)
            .send()
            .await
            .map_err(|source| HarnessError::Transport {
                url: url.clone(),
                source,
            })?;
        let status = resp.status();
        let text = resp.text().await.map_err(|source| HarnessError::Transport {
            url: url.clone(),
            source,
        })?;
        if !status.is_success() {
            return Err(HarnessError::HttpStatus {
                method: "POST".into(),
                url,
                status: status.as_u16(),
                body: text,
            });
        }
        Ok(text)
    }
}

#[async_trait]
impl GameClient for HttpGameClient {
    fn base_url(&self) -> &str {
        &self.base_url
    }

    async fn health(&self) -> Result<()> {
        // Health may return any small body; we only care that it is 2xx.
        let url = self.url("/api/health");
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|source| HarnessError::Transport {
                url: url.clone(),
                source,
            })?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(HarnessError::HttpStatus {
                method: "GET".into(),
                url,
                status: resp.status().as_u16(),
                body: resp.text().await.unwrap_or_default(),
            })
        }
    }

    async fn new_game(&self) -> Result<()> {
        // Body-less mutation: send `{}` (POST) like parish_new_game does.
        self.post_text("/api/new-game", &json!({})).await.map(|_| ())
    }

    async fn submit_command(
        &self,
        text: &str,
        addressed_to: &[String],
        timeout_ms: u64,
    ) -> Result<CommandResponse> {
        let body = json!({
            "text": text,
            "addressedTo": addressed_to,
            "timeoutMs": timeout_ms,
            "includeState": true,
        });
        let raw = self.post_text("/api/command", &body).await?;
        serde_json::from_str(&raw).map_err(|source| HarnessError::Json {
            context: format!("POST /api/command (text={text:?})"),
            source,
        })
    }

    async fn engine_state(&self) -> Result<EngineState> {
        self.get_json("/api/engine-state").await
    }

    async fn apply_flag(&self, name: &str, on: bool) -> Result<()> {
        let verb = if on { "on" } else { "off" };
        let _ = self
            .submit_command(&format!("/flag {name} {verb}"), &[], 30_000)
            .await?;
        Ok(())
    }

    async fn apply_byok(&self, category: &str, model: &EngineModel) -> Result<()> {
        let mut body = json!({
            "category": category,
            "provider": model.provider,
            "model": model.model,
        });
        if let Some(base_url) = &model.base_url {
            body["base_url"] = json!(base_url);
        }
        self.post_text("/api/submit-byok", &body).await.map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn submit_command_parses_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/command"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "outcome": "ok",
                "kind": "looked",
                "echo": "look",
                "lines": [{"id": "1", "role": "system", "speaker": "System", "text": "A muddy lane."}],
                "kind_detail": {},
                "elapsed_ms": 5
            })))
            .mount(&server)
            .await;

        let client = HttpGameClient::new(server.uri());
        let resp = client.submit_command("look", &[], 30_000).await.unwrap();
        assert_eq!(resp.outcome(), crate::client::wire::Outcome::Ok);
        assert_eq!(resp.kind, "looked");
        assert_eq!(resp.lines[0].text, "A muddy lane.");
    }

    #[tokio::test]
    async fn health_ok_on_2xx() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/health"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&server)
            .await;
        let client = HttpGameClient::new(server.uri());
        assert!(client.health().await.is_ok());
    }

    #[tokio::test]
    async fn non_2xx_is_http_status_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/new-game"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;
        let client = HttpGameClient::new(server.uri());
        let err = client.new_game().await.unwrap_err();
        assert!(matches!(err, HarnessError::HttpStatus { status: 500, .. }));
    }

    #[tokio::test]
    async fn transport_error_when_nothing_listening() {
        // Reserve-and-free a port to guarantee connection-refused.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let client = HttpGameClient::new(format!("http://{addr}"));
        let err = client.new_game().await.unwrap_err();
        assert!(matches!(err, HarnessError::Transport { .. }));
    }
}
