//! Browser-bridge inference client for the [`Provider::WebGpu`] backend.
//!
//! Unlike every other client in this crate, no HTTP request leaves the
//! server: the prompt is forwarded over the calling browser's existing
//! WebSocket, the browser runs the model locally via `transformers.js` on
//! WebGPU, and tokens stream back the same way. The actual transport is
//! injected by [`parish-server`] (which owns the WebSocket); this crate
//! only defines the transport trait and the [`WebGpuClient`] handle.
//!
//! In Tauri or headless CLI builds no transport is ever attached, so a
//! [`WebGpuClient::unavailable`] handle is returned instead. Calling
//! `generate*` on an unavailable handle returns a clear error rather than
//! silently misrouting the call to a different provider.
//!
//! [`Provider::WebGpu`]: parish_config::Provider::WebGpu

use std::sync::Arc;

use parish_types::ParishError;
use serde::de::DeserializeOwned;
use tokio::sync::{mpsc, oneshot};

/// A single browser-side inference request sent over the bridge.
///
/// `token_tx` is `Some` for streaming requests; the bridge forwards each
/// `webgpu-token` frame received from the browser into this channel before
/// finally resolving the response oneshot with the full text.
#[derive(Debug)]
pub struct WebGpuRequest {
    /// Hugging Face repo id of the ONNX model the browser should run
    /// (e.g. `onnx-community/gemma-4-E2B-it-ONNX`). The browser may further
    /// override this if the user has pinned a model in `localStorage`.
    pub model: String,
    /// User prompt text.
    pub prompt: String,
    /// Optional system prompt. The browser concatenates this with the user
    /// prompt using the model's chat template.
    pub system: Option<String>,
    /// Maximum number of new tokens to generate. `None` defers to the
    /// browser's per-model default.
    pub max_tokens: Option<u32>,
    /// Sampling temperature. `None` defers to the browser's default.
    pub temperature: Option<f32>,
    /// When `true`, the browser is asked to generate JSON-mode output
    /// (currently a soft hint — `transformers.js` does not enforce schema,
    /// so callers should still validate the returned text with `serde_json`).
    pub json_mode: bool,
    /// Optional channel into which the bridge forwards streamed tokens.
    /// `None` means "non-streaming" — only the final `webgpu-end` frame
    /// resolves the response.
    pub token_tx: Option<mpsc::UnboundedSender<String>>,
}

/// Submission interface for browser-side inference, implemented by the
/// per-session bridge in [`parish-server`].
///
/// Returning a `oneshot::Receiver` rather than an async fn keeps this trait
/// `dyn`-compatible without depending on `async_trait` or stable RPITIT.
pub trait WebGpuTransport: Send + Sync + std::fmt::Debug {
    /// Submits `req` to the connected browser and returns a one-shot
    /// receiver that resolves with the final response (or an error).
    ///
    /// For streaming requests the bridge will additionally forward
    /// per-token deltas into `req.token_tx` before resolving the oneshot.
    fn submit(&self, req: WebGpuRequest) -> oneshot::Receiver<Result<String, ParishError>>;
}

/// Client handle for the [`Provider::WebGpu`] backend.
///
/// In the web server build a real [`WebGpuTransport`] is attached per
/// session. In other builds (Tauri, headless CLI) the handle is constructed
/// via [`WebGpuClient::unavailable`] and every `generate*` call returns a
/// clear `ParishError::Config` so the misuse surfaces immediately.
///
/// [`Provider::WebGpu`]: parish_config::Provider::WebGpu
#[derive(Clone, Debug)]
pub struct WebGpuClient {
    transport: Option<Arc<dyn WebGpuTransport>>,
}

impl WebGpuClient {
    /// Builds a client that forwards every request to `transport`.
    pub fn with_transport(transport: Arc<dyn WebGpuTransport>) -> Self {
        Self {
            transport: Some(transport),
        }
    }

    /// Builds a no-op client that errors on every request.
    ///
    /// Used by `build_client` in non-web builds (Tauri, CLI) so that
    /// selecting `Provider::WebGpu` produces a clear error path instead
    /// of silently routing through the wrong backend.
    pub fn unavailable() -> Self {
        Self { transport: None }
    }

    /// Returns whether a transport has been attached.
    pub fn is_available(&self) -> bool {
        self.transport.is_some()
    }

    fn transport(&self) -> Result<&Arc<dyn WebGpuTransport>, ParishError> {
        self.transport.as_ref().ok_or_else(|| {
            ParishError::Config(
                "WebGPU provider is browser-only; switch to a different provider in this build."
                    .to_string(),
            )
        })
    }

    /// Generates plain text (non-streaming).
    pub async fn generate(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
    ) -> Result<String, ParishError> {
        let rx = self.transport()?.submit(WebGpuRequest {
            model: model.to_string(),
            prompt: prompt.to_string(),
            system: system.map(str::to_string),
            max_tokens,
            temperature,
            json_mode: false,
            token_tx: None,
        });
        match rx.await {
            Ok(result) => result,
            Err(_) => Err(ParishError::Inference(
                "WebGPU bridge dropped before responding".to_string(),
            )),
        }
    }

    /// Generates text with token streaming.
    ///
    /// Tokens are forwarded into `token_tx` as `webgpu-token` frames arrive
    /// from the browser. The final assembled text is also returned.
    pub async fn generate_stream(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        token_tx: mpsc::UnboundedSender<String>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
    ) -> Result<String, ParishError> {
        let rx = self.transport()?.submit(WebGpuRequest {
            model: model.to_string(),
            prompt: prompt.to_string(),
            system: system.map(str::to_string),
            max_tokens,
            temperature,
            json_mode: false,
            token_tx: Some(token_tx),
        });
        match rx.await {
            Ok(result) => result,
            Err(_) => Err(ParishError::Inference(
                "WebGPU bridge dropped before responding".to_string(),
            )),
        }
    }

    /// Generates a structured JSON response and deserializes it into `T`.
    ///
    /// `transformers.js` does not enforce a schema, so we just request a
    /// JSON-mode hint and rely on `serde_json` to validate the result.
    pub async fn generate_json<T: DeserializeOwned>(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
    ) -> Result<T, ParishError> {
        let rx = self.transport()?.submit(WebGpuRequest {
            model: model.to_string(),
            prompt: prompt.to_string(),
            system: system.map(str::to_string),
            max_tokens,
            temperature,
            json_mode: true,
            token_tx: None,
        });
        let text = match rx.await {
            Ok(result) => result?,
            Err(_) => {
                return Err(ParishError::Inference(
                    "WebGPU bridge dropped before responding".to_string(),
                ));
            }
        };
        serde_json::from_str(&text)
            .map_err(|e| ParishError::Inference(format!("WebGPU JSON parse failed: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal in-process transport used to verify the client wiring.
    #[derive(Debug)]
    struct EchoTransport {
        /// Optional canned token stream emitted before the final response.
        stream_tokens: Vec<String>,
    }

    impl WebGpuTransport for EchoTransport {
        fn submit(&self, req: WebGpuRequest) -> oneshot::Receiver<Result<String, ParishError>> {
            let (tx, rx) = oneshot::channel();
            let stream_tokens = self.stream_tokens.clone();
            tokio::spawn(async move {
                if let Some(token_tx) = req.token_tx.as_ref() {
                    for tok in &stream_tokens {
                        let _ = token_tx.send(tok.clone());
                    }
                }
                let body = format!("echo[{}]: {}", req.model, req.prompt);
                let _ = tx.send(Ok(body));
            });
            rx
        }
    }

    #[tokio::test]
    async fn generate_round_trips_through_transport() {
        let client = WebGpuClient::with_transport(Arc::new(EchoTransport {
            stream_tokens: Vec::new(),
        }));
        let out = client
            .generate("gemma-4-E2B", "hello", None, None, None)
            .await
            .unwrap();
        assert_eq!(out, "echo[gemma-4-E2B]: hello");
    }

    #[tokio::test]
    async fn generate_stream_forwards_tokens() {
        let client = WebGpuClient::with_transport(Arc::new(EchoTransport {
            stream_tokens: vec!["alpha".into(), "beta".into(), "gamma".into()],
        }));
        let (tx, mut rx) = mpsc::unbounded_channel();
        let final_text = client
            .generate_stream("gemma-4-E2B", "hi", None, tx, None, None)
            .await
            .unwrap();
        assert_eq!(final_text, "echo[gemma-4-E2B]: hi");
        let mut got = Vec::new();
        while let Ok(tok) = rx.try_recv() {
            got.push(tok);
        }
        assert_eq!(got, vec!["alpha", "beta", "gamma"]);
    }

    #[tokio::test]
    async fn unavailable_client_errors_with_config_message() {
        let client = WebGpuClient::unavailable();
        let err = client
            .generate("any", "prompt", None, None, None)
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("browser-only"), "got: {msg}");
    }

    #[tokio::test]
    async fn unavailable_client_streaming_errors() {
        let client = WebGpuClient::unavailable();
        let (tx, _rx) = mpsc::unbounded_channel();
        assert!(
            client
                .generate_stream("any", "prompt", None, tx, None, None)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn dropped_transport_yields_clear_error() {
        #[derive(Debug)]
        struct DropTransport;
        impl WebGpuTransport for DropTransport {
            fn submit(
                &self,
                _req: WebGpuRequest,
            ) -> oneshot::Receiver<Result<String, ParishError>> {
                let (_tx, rx) = oneshot::channel();
                // tx is dropped immediately, so rx will yield a RecvError.
                rx
            }
        }
        let client = WebGpuClient::with_transport(Arc::new(DropTransport));
        let err = client
            .generate("m", "p", None, None, None)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("dropped"), "got: {err}");
    }
}
