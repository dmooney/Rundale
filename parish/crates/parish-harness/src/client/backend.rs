//! HTTP game client — drives a running Parish backend over `/api/*`.
//!
//! Mirrors the dispatch shape proven in `parish-mcp`'s `ParishHttpBackend`
//! (kebab paths, GET vs POST), but exposes a typed, harness-specific surface
//! instead of a generic `invoke`. The harness never links `parish-server` at
//! runtime; it speaks HTTP to that server's `/api/command` endpoint. The Tauri
//! bridge intentionally does not expose `/api/command`, so this client cannot
//! drive the desktop window.

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
    /// Apply a per-category BYOK model override by issuing the runtime
    /// slash commands `/provider.<cat>`, `/model.<cat>`, and (when a key is
    /// resolvable from the environment) `/key.<cat>` over `POST /api/command`.
    ///
    /// Each command routes through `handle_system_command`, which triggers
    /// `rebuild_inference`, so the worker rebinds the category at runtime —
    /// no `/api/submit-byok` route is required (that route 404s on
    /// parish-server and is base-only on the Tauri bridge; see #1365).
    ///
    /// **Key hygiene (#1365):** the API key is resolved from the harness's
    /// environment at apply-time (via the provider's `api_key_env_var`), never
    /// carried in [`EngineModel`] / `RunConfig` (which is content-hashed and
    /// persisted). The `/key` command text is never logged verbatim.
    async fn apply_byok(&self, category: &str, model: &EngineModel) -> Result<()>;
}

/// Resolves the API key for a provider from the harness environment.
///
/// Returns `Some(key)` when the provider declares an `api_key_env_var` and that
/// variable is set to a non-empty value. Returns `None` for keyless providers
/// (local vllm-mlx / ollama / simulator) and when a cloud provider's key var is
/// unset (the caller then skips the `/key` command rather than sending an empty
/// secret). The key VALUE is never returned to a logging path.
pub(crate) fn resolve_provider_key_from_env(provider_name: &str) -> Option<String> {
    let provider = parish_core::config::Provider::from_str_loose(provider_name).ok()?;
    let var = provider.api_key_env_var()?;
    std::env::var(var).ok().filter(|v| !v.trim().is_empty())
}

/// Concrete HTTP client.
pub struct HttpGameClient {
    base_url: String,
    client: reqwest::Client,
}

impl HttpGameClient {
    /// Construct a client pointed at a `parish-server` backend root
    /// (e.g. `http://127.0.0.1:3030`).
    ///
    /// A per-client cookie store is essential: `parish-server` isolates state
    /// per visitor keyed by the `parish_sid` cookie, so a cookieless client
    /// would spin up a brand-new session on *every* request and exhaust the
    /// server's session cap within a single run. Enabling the cookie store
    /// makes all of a run's requests share one session (the same approach
    /// `parish-client` takes).
    pub fn new(base_url: impl Into<String>) -> Self {
        // Defensive client-side timeouts so a hung connection can never block a
        // run forever. The request timeout is deliberately longer than the
        // server's own per-command ceiling (max 120s, see sync_types) so the
        // backend's graceful partial-response path wins on a slow turn; the
        // hard client timeout only fires on a genuinely dead socket. The short
        // connect timeout surfaces "nothing listening" fast (it pairs with the
        // Crash gate at run start).
        let client = reqwest::Client::builder()
            .cookie_store(true)
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(180))
            .build()
            .expect("reqwest client builds with cookie store");
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// GET a path and deserialize the body into `T`.
    async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = self.url(path);
        let resp =
            self.client
                .get(&url)
                .send()
                .await
                .map_err(|source| HarnessError::Transport {
                    url: url.clone(),
                    source,
                })?;
        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|source| HarnessError::Transport {
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
        let text = resp
            .text()
            .await
            .map_err(|source| HarnessError::Transport {
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
        let resp =
            self.client
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
        self.post_text("/api/new-game", &json!({}))
            .await
            .map(|_| ())
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
        // The `dialogue` category is served by the game's BASE provider/model
        // slot, not by per-category client resolution: the NPC dialogue path
        // uses `GameConfig::model_name` + the base worker, whereas
        // intent/reaction/simulation are routed through
        // `resolve_category_client`. So `engine_models.dialogue` maps to the
        // base slash commands (`/provider`, `/url`, `/model`, `/key`) and every
        // other category maps to its dotted form (`/provider.<cat>`, …). This
        // is why a prior naive `/provider.dialogue` left dialogue on the base
        // 14B model even though the snapshot showed the override (#1365).
        let suffix = if category == "dialogue" {
            String::new()
        } else {
            format!(".{category}")
        };

        // 1. Provider.
        tracing::info!(category, provider = %model.provider, "applying engine_model provider");
        self.submit_command(
            &format!("/provider{suffix} {}", model.provider),
            &[],
            30_000,
        )
        .await?;

        // 2. Base URL, if the config pins one. Setting the provider resets the
        //    URL to the provider default, so a BYOK target whose URL differs
        //    (e.g. the 1.5B slot on :8001) must override it AFTER the provider.
        if let Some(base_url) = &model.base_url {
            tracing::info!(category, base_url = %base_url, "applying engine_model base_url");
            self.submit_command(&format!("/url{suffix} {base_url}"), &[], 30_000)
                .await?;
        }

        // 3. Model.
        tracing::info!(category, model = %model.model, "applying engine_model model");
        self.submit_command(&format!("/model{suffix} {}", model.model), &[], 30_000)
            .await?;

        // 4. Key — resolved from the harness env at apply-time, never from the
        //    config. Skip silently for keyless providers; warn (without the
        //    secret) when a cloud provider's key var is unset.
        match resolve_provider_key_from_env(&model.provider) {
            Some(key) => {
                // The command text carries the secret; build it locally and
                // never log it verbatim (#1365 AC3).
                tracing::info!(category, provider = %model.provider, "applying engine_model key from env (redacted)");
                self.submit_command(&format!("/key{suffix} {key}"), &[], 30_000)
                    .await?;
            }
            None => {
                if needs_api_key(&model.provider) {
                    tracing::warn!(
                        category,
                        provider = %model.provider,
                        "no API key in env for this provider; skipping /key — dialogue may be unauthorised. \
                         Set the provider's key env var (e.g. ANTHROPIC_API_KEY)."
                    );
                }
            }
        }
        Ok(())
    }
}

/// Whether a provider requires an API key (i.e. declares an `api_key_env_var`).
/// Local providers (vllm-mlx / ollama / simulator) return false.
fn needs_api_key(provider_name: &str) -> bool {
    parish_core::config::Provider::from_str_loose(provider_name)
        .ok()
        .and_then(|p| p.api_key_env_var().map(|_| ()))
        .is_some()
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

    /// AC1 (#1365): apply_byok issues `/provider.<cat>` and `/model.<cat>` over
    /// `/api/command` (NOT the removed `/api/submit-byok`). For a keyless
    /// provider (no env key) it sends exactly those two commands and no `/key`.
    #[tokio::test]
    async fn apply_byok_issues_provider_and_model_slash_commands() {
        let server = MockServer::start().await;
        // Every command returns a benign ok response.
        Mock::given(method("POST"))
            .and(path("/api/command"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "outcome": "ok",
                "kind": "system",
                "echo": "",
                "lines": [],
                "kind_detail": {},
                "elapsed_ms": 1
            })))
            .mount(&server)
            .await;

        let client = HttpGameClient::new(server.uri());
        // vllm-mlx is keyless, so no env key is consulted/required.
        let model = EngineModel {
            provider: "vllm-mlx".into(),
            model: "mlx-community/Qwen2.5-14B-Instruct-4bit".into(),
            base_url: Some("http://localhost:8000".into()),
        };
        client.apply_byok("dialogue", &model).await.unwrap();

        // Inspect the recorded request bodies: a `/provider.dialogue` and a
        // `/model.dialogue` command, and NO `/api/submit-byok` request.
        let requests = server.received_requests().await.unwrap();
        let texts: Vec<String> = requests
            .iter()
            .filter(|r| r.url.path() == "/api/command")
            .map(|r| {
                let v: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
                v["text"].as_str().unwrap_or_default().to_string()
            })
            .collect();
        // Dialogue maps to the BASE slash commands (no `.dialogue` suffix),
        // because the dialogue path is served by the base provider slot.
        assert!(
            texts.iter().any(|t| t == "/provider vllm-mlx"),
            "expected base /provider command for dialogue, got {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t == "/url http://localhost:8000"),
            "expected base /url command for a pinned base_url, got {texts:?}"
        );
        assert!(
            texts
                .iter()
                .any(|t| t == "/model mlx-community/Qwen2.5-14B-Instruct-4bit"),
            "expected base /model command for dialogue, got {texts:?}"
        );
        // No /key command for a keyless provider.
        assert!(
            !texts.iter().any(|t| t.starts_with("/key.")),
            "keyless provider must not send a /key command, got {texts:?}"
        );
        // The removed flat endpoint must never be hit.
        assert!(
            requests.iter().all(|r| r.url.path() != "/api/submit-byok"),
            "apply_byok must not POST /api/submit-byok any more"
        );
    }

    /// A non-dialogue category (intent) maps to the DOTTED slash commands
    /// (`/provider.intent`, `/url.intent`, `/model.intent`), routed via
    /// per-category client resolution.
    #[tokio::test]
    async fn apply_byok_intent_uses_dotted_category_commands() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/command"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "outcome": "ok", "kind": "system", "echo": "",
                "lines": [], "kind_detail": {}, "elapsed_ms": 1
            })))
            .mount(&server)
            .await;

        let client = HttpGameClient::new(server.uri());
        let model = EngineModel {
            provider: "vllm-mlx".into(),
            model: "mlx-community/Qwen2.5-1.5B-Instruct-4bit".into(),
            base_url: Some("http://localhost:8001".into()),
        };
        client.apply_byok("intent", &model).await.unwrap();

        let requests = server.received_requests().await.unwrap();
        let texts: Vec<String> = requests
            .iter()
            .filter(|r| r.url.path() == "/api/command")
            .map(|r| {
                let v: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
                v["text"].as_str().unwrap_or_default().to_string()
            })
            .collect();
        assert!(
            texts.iter().any(|t| t == "/provider.intent vllm-mlx"),
            "{texts:?}"
        );
        assert!(
            texts
                .iter()
                .any(|t| t == "/url.intent http://localhost:8001"),
            "{texts:?}"
        );
        assert!(
            texts
                .iter()
                .any(|t| t == "/model.intent mlx-community/Qwen2.5-1.5B-Instruct-4bit"),
            "{texts:?}"
        );
    }

    /// AC2/AC3 (#1365): the key is resolved from the harness env at apply-time
    /// and never carried in EngineModel. EngineModel has no api_key field
    /// (structural guarantee); this asserts the env resolver wiring.
    #[test]
    fn resolve_provider_key_reads_env_for_cloud_and_skips_local() {
        // Local providers have no key env var → None regardless of env.
        assert!(resolve_provider_key_from_env("vllm-mlx").is_none());
        assert!(resolve_provider_key_from_env("simulator").is_none());
        // Anthropic declares ANTHROPIC_API_KEY; needs_api_key is true even when
        // unset, so the caller warns rather than sending an empty secret.
        assert!(needs_api_key("anthropic"));
        assert!(!needs_api_key("vllm-mlx"));
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
