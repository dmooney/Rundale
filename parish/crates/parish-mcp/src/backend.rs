//! Tauri-app control backends.
//!
//! [`TauriBackend`] is the seam between the protocol layer and whatever
//! is actually being driven. Two impls live here:
//!
//! - [`ParishHttpBackend`] — talks HTTP to a running `parish-server`. Because
//!   `parish-server`'s IPC routes are mode-parity with the Tauri commands
//!   (enforced by `parish-core/tests/wiring_parity.rs`), this drives the same
//!   game logic as the desktop app would. This is the recommended path: it
//!   isolates the MCP server from the GTK/wry build, runs against any
//!   environment that can host an Axum process, and reuses the parity sensor
//!   for free.
//!
//! - [`GenericTauriBackend`] — a stub for the future WebDriver / `tauri-driver`
//!   path that drives an arbitrary Tauri app's window directly. It returns
//!   [`BackendError::Unimplemented`] today; the type is exported so downstream
//!   code can pin its tool registry against the trait now and the real impl
//!   can land later without API churn.

use async_trait::async_trait;
use serde_json::Value;

/// Errors returned by a [`TauriBackend`]. Mapped onto JSON-RPC errors by the
/// MCP layer.
#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("transport error: {0}")]
    Transport(String),
    #[error("backend rejected the request: {0}")]
    Rejected(String),
    #[error("backend not implemented: {0}")]
    Unimplemented(&'static str),
}

/// Operations any Tauri-app controller must support to be useful for MCP.
///
/// Kept narrow on purpose: the protocol layer should not depend on
/// Parish-specific types. Higher-level tools (e.g. `parish_submit_input`)
/// compose calls to [`Self::invoke`] internally.
#[async_trait]
pub trait TauriBackend: Send + Sync {
    /// Returns a short identifier used in tool descriptions and logs
    /// (e.g. `"parish-http"`).
    fn name(&self) -> &'static str;

    /// Invokes a named command on the backing Tauri app and returns the
    /// raw JSON response. The semantics of `command` depend on the backend:
    /// for [`ParishHttpBackend`] it is the canonical command name (which is
    /// translated to `/api/<kebab-case>`); for [`GenericTauriBackend`] it
    /// will be the Tauri IPC command name passed to `invoke()` in the
    /// webview.
    async fn invoke(&self, command: &str, args: Value) -> Result<Value, BackendError>;
}

// ── ParishHttpBackend ────────────────────────────────────────────────────────

/// Drives a local or remote `parish-server` over HTTP.
///
/// `base_url` should point at the Axum root (e.g. `http://127.0.0.1:3030`).
/// `auth_token`, if set, is sent as a `Cf-Access-Authenticated-User-Email`
/// header so a server protected by Cloudflare Access can be addressed in
/// dev (the loopback bypass also covers most local-dev cases).
pub struct ParishHttpBackend {
    base_url: String,
    auth_email: Option<String>,
    client: reqwest::Client,
}

impl ParishHttpBackend {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            auth_email: None,
            client: reqwest::Client::new(),
        }
    }

    /// Sets the email forwarded as `Cf-Access-Authenticated-User-Email`.
    pub fn with_auth_email(mut self, email: impl Into<String>) -> Self {
        self.auth_email = Some(email.into());
        self
    }

    /// Translates a Tauri-style command name to the kebab-cased path under
    /// `/api/`. Visible-for-test so the conversion can be pinned.
    pub fn command_to_path(command: &str) -> String {
        // Strip an optional leading `get_` so `get_world_snapshot` and
        // `world-snapshot` both map to `/api/world-snapshot` — matches the
        // behaviour of `parish-core/tests/wiring_parity.rs::tauri_to_canonical`.
        let stem = command.strip_prefix("get_").unwrap_or(command);
        let kebab: String = stem
            .chars()
            .map(|c| {
                if c == '_' {
                    '-'
                } else {
                    c.to_ascii_lowercase()
                }
            })
            .collect();
        format!("/api/{kebab}")
    }

    /// Heuristic: GET when `args` is JSON null, POST otherwise.
    ///
    /// Empty objects (`{}`) are POST: tool translators like
    /// `parish_new_game` and `parish_save_game` deliberately emit
    /// `json!({})` to signal a body-less mutation endpoint, and the
    /// matching `parish-server` route is registered with `.post(...)`.
    /// Treating `{}` as GET would silently route those calls to a 404.
    fn is_post(args: &Value) -> bool {
        !args.is_null()
    }
}

#[async_trait]
impl TauriBackend for ParishHttpBackend {
    fn name(&self) -> &'static str {
        "parish-http"
    }

    async fn invoke(&self, command: &str, args: Value) -> Result<Value, BackendError> {
        let url = format!("{}{}", self.base_url, Self::command_to_path(command));
        let use_post = Self::is_post(&args);
        let mut req = if use_post {
            self.client.post(&url).json(&args)
        } else {
            self.client.get(&url)
        };
        if let Some(email) = &self.auth_email {
            req = req.header("Cf-Access-Authenticated-User-Email", email);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| BackendError::Transport(e.to_string()))?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| BackendError::Transport(e.to_string()))?;

        if !status.is_success() {
            return Err(BackendError::Rejected(format!(
                "{} {url} -> {status}: {body}",
                if use_post { "POST" } else { "GET" },
            )));
        }

        // Empty body (e.g. submit-input returns `200 OK` with no JSON) is
        // surfaced as `null` rather than an error so tools can still
        // succeed without a result payload.
        if body.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&body).map_err(|e| {
            BackendError::Transport(format!("invalid JSON from {url}: {e} (body: {body})"))
        })
    }
}

// ── GenericTauriBackend (future) ────────────────────────────────────────────

/// Placeholder for a generic, app-agnostic Tauri controller backed by
/// WebDriver / `tauri-driver`. Returns [`BackendError::Unimplemented`] until
/// the implementation lands.
///
/// Kept as a real type (rather than a `// TODO`) so the MCP server can be
/// wired against `Box<dyn TauriBackend>` end-to-end and the day this lands
/// nothing else has to change.
pub struct GenericTauriBackend;

#[async_trait]
impl TauriBackend for GenericTauriBackend {
    fn name(&self) -> &'static str {
        "generic-tauri-driver"
    }

    async fn invoke(&self, _command: &str, _args: Value) -> Result<Value, BackendError> {
        Err(BackendError::Unimplemented(
            "GenericTauriBackend (WebDriver/tauri-driver) is not implemented yet",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn command_to_path_matches_wiring_parity_translation() {
        assert_eq!(
            ParishHttpBackend::command_to_path("get_world_snapshot"),
            "/api/world-snapshot"
        );
        assert_eq!(
            ParishHttpBackend::command_to_path("world_snapshot"),
            "/api/world-snapshot"
        );
        assert_eq!(
            ParishHttpBackend::command_to_path("submit_input"),
            "/api/submit-input"
        );
        assert_eq!(
            ParishHttpBackend::command_to_path("editor_open_mod"),
            "/api/editor-open-mod"
        );
    }

    #[test]
    fn is_post_treats_null_as_get_and_everything_else_as_post() {
        assert!(!ParishHttpBackend::is_post(&Value::Null));
        // Empty objects must be POST: parish_new_game / parish_save_game emit
        // `json!({})` to drive a body-less mutation endpoint.
        assert!(ParishHttpBackend::is_post(&serde_json::json!({})));
        assert!(ParishHttpBackend::is_post(
            &serde_json::json!({"text": "hi"})
        ));
        assert!(ParishHttpBackend::is_post(&serde_json::json!([1, 2, 3])));
    }

    /// Regression: empty-object args (the shape `parish_new_game` / `parish_save_game`
    /// emit) must reach the backend as POST, not GET. Pre-fix this test would
    /// fail because `is_post(&json!({}))` returned `false`.
    #[tokio::test]
    async fn empty_object_args_dispatch_as_post() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/new-game"))
            .respond_with(ResponseTemplate::new(200).set_body_string(""))
            .mount(&server)
            .await;

        let backend = ParishHttpBackend::new(server.uri());
        let v = backend
            .invoke("new_game", serde_json::json!({}))
            .await
            .unwrap();
        assert!(v.is_null());
    }

    /// Failures on a GET request must report `GET` in the error message —
    /// the pre-fix code hardcoded `POST` because it called
    /// `is_post(&Value::Null)` instead of inspecting the actual args.
    #[tokio::test]
    async fn rejected_error_reports_actual_http_method() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/world-snapshot"))
            .respond_with(ResponseTemplate::new(404).set_body_string("missing"))
            .mount(&server)
            .await;
        let backend = ParishHttpBackend::new(server.uri());
        let err = backend
            .invoke("get_world_snapshot", Value::Null)
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("GET "), "expected GET in error, got: {msg}");
    }

    #[tokio::test]
    async fn http_get_returns_parsed_json() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/world-snapshot"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"clock": 42})),
            )
            .mount(&server)
            .await;

        let backend = ParishHttpBackend::new(server.uri());
        let v = backend
            .invoke("get_world_snapshot", Value::Null)
            .await
            .unwrap();
        assert_eq!(v["clock"], 42);
    }

    #[tokio::test]
    async fn http_post_forwards_body_and_auth_header() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/submit-input"))
            .and(header(
                "cf-access-authenticated-user-email",
                "dev@example.com",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(""))
            .mount(&server)
            .await;

        let backend = ParishHttpBackend::new(server.uri()).with_auth_email("dev@example.com");
        let v = backend
            .invoke("submit_input", serde_json::json!({"text": "look"}))
            .await
            .unwrap();
        // Empty body → null result.
        assert!(v.is_null());
    }

    #[tokio::test]
    async fn http_non_2xx_is_surfaced_as_rejected() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/world-snapshot"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;
        let backend = ParishHttpBackend::new(server.uri());
        let err = backend
            .invoke("get_world_snapshot", Value::Null)
            .await
            .unwrap_err();
        assert!(matches!(err, BackendError::Rejected(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn generic_backend_is_unimplemented() {
        let backend = GenericTauriBackend;
        let err = backend.invoke("anything", Value::Null).await.unwrap_err();
        assert!(matches!(err, BackendError::Unimplemented(_)));
    }
}
