//! Per-session WebGPU bridge — forwards inference requests to the calling
//! browser over the existing WebSocket and routes streaming tokens / final
//! responses back to the in-process [`WebGpuClient`].
//!
//! # Wire protocol
//!
//! Server → browser frames are emitted on the per-session [`EventBus`] as
//! standard `ServerEvent`s with `event = "webgpu-generate"` and a payload of
//! [`WebGpuGenerateFrame`]. The browser-side bridge subscribes to that
//! event, runs `transformers.js` on WebGPU, and writes the following frames
//! back over the WebSocket as `{event, payload}` JSON envelopes:
//!
//! - `webgpu-token`  →  [`WebGpuTokenFrame`]: one or more streamed tokens.
//! - `webgpu-end`    →  [`WebGpuEndFrame`]: terminal frame with the full
//!   text, resolves the in-process oneshot.
//! - `webgpu-error`  →  [`WebGpuErrorFrame`]: terminal frame with an error
//!   message; rejects the oneshot.
//!
//! The server matches replies to in-flight requests by `request_id`, a
//! monotonic counter scoped to the bridge instance.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use dashmap::DashMap;
use parish_core::error::ParishError;
use parish_core::inference::{WebGpuRequest, WebGpuTransport};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

use crate::state::{EventBus, ServerEvent};

/// Shared default model when neither the user nor the browser overrides
/// it. Picked to match the linked `webml-community/Gemma-4-WebGPU` demo
/// (Gemma 4 ~2B, q4 ≈ 1.5 GB) so first-time loads are reasonably small
/// while still high-quality. The browser may still upgrade to E4B if its
/// GPU tier detection picks the larger model.
pub const DEFAULT_WEBGPU_MODEL: &str = "onnx-community/gemma-4-E2B-it-ONNX";

/// Server → browser request frame (carried by the `webgpu-generate` event).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct WebGpuGenerateFrame {
    /// Monotonic id used to correlate replies. Scoped to a single bridge.
    pub request_id: u64,
    /// Hugging Face repo id of the ONNX model to run.
    pub model: String,
    /// User prompt text.
    pub prompt: String,
    /// Optional system prompt. Browsers concatenate this with the user
    /// prompt using the model's chat template.
    pub system: Option<String>,
    /// Optional max-new-tokens cap. `None` defers to the model's default.
    pub max_tokens: Option<u32>,
    /// Optional sampling temperature.
    pub temperature: Option<f32>,
    /// Whether the browser should stream tokens (else only the final
    /// `webgpu-end` frame is sent).
    pub streaming: bool,
    /// Soft hint that the browser should bias toward valid JSON output.
    pub json_mode: bool,
}

/// Browser → server: one (or more) streamed tokens.
#[derive(Debug, Clone, Deserialize)]
pub struct WebGpuTokenFrame {
    pub request_id: u64,
    pub delta: String,
}

/// Browser → server: terminal success frame.
#[derive(Debug, Clone, Deserialize)]
pub struct WebGpuEndFrame {
    pub request_id: u64,
    /// Full assembled text (the bridge prefers this over re-concatenating
    /// `webgpu-token` deltas in case the browser had to repair mid-stream).
    pub full_text: String,
}

/// Browser → server: terminal error frame.
#[derive(Debug, Clone, Deserialize)]
pub struct WebGpuErrorFrame {
    pub request_id: u64,
    pub message: String,
}

/// Per-request state held by the bridge while a browser-side generation
/// is in flight.
struct PendingRequest {
    /// Where to send the final assembled text (or terminal error).
    response_tx: oneshot::Sender<Result<String, ParishError>>,
    /// Where to forward streamed tokens (`None` for non-streaming requests).
    token_tx: Option<mpsc::UnboundedSender<String>>,
}

/// The actual transport implementation injected into [`WebGpuClient`].
///
/// Cheap to clone (everything sits behind `Arc`s); each clone observes the
/// same in-flight request map so frame routing always lands on the right
/// pending channel.
#[derive(Debug, Clone)]
pub struct WebGpuBridge {
    inner: Arc<BridgeInner>,
}

struct BridgeInner {
    event_bus: EventBus,
    next_id: AtomicU64,
    pending: DashMap<u64, PendingRequest>,
}

impl std::fmt::Debug for BridgeInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BridgeInner")
            .field("next_id", &self.next_id.load(Ordering::Relaxed))
            .field("pending_count", &self.pending.len())
            .finish()
    }
}

impl WebGpuBridge {
    /// Builds a fresh bridge bound to `event_bus`. Each session should own
    /// one bridge — sharing a bridge across sessions would let one user's
    /// browser receive another user's prompts.
    pub fn new(event_bus: EventBus) -> Self {
        Self {
            inner: Arc::new(BridgeInner {
                event_bus,
                next_id: AtomicU64::new(1),
                pending: DashMap::new(),
            }),
        }
    }

    /// Routes a `webgpu-token` frame from the WebSocket to the matching
    /// in-flight request's token channel. Drops silently if no request
    /// matches (e.g. late frame after the request was already cancelled).
    pub fn handle_token(&self, frame: WebGpuTokenFrame) {
        if let Some(entry) = self.inner.pending.get(&frame.request_id)
            && let Some(tx) = entry.token_tx.as_ref()
        {
            // Receiver may have been dropped by an aborted caller; ignore.
            let _ = tx.send(frame.delta);
        }
    }

    /// Resolves a `webgpu-end` frame's matching pending request with the
    /// final text and removes it from the pending map.
    pub fn handle_end(&self, frame: WebGpuEndFrame) {
        if let Some((_, pending)) = self.inner.pending.remove(&frame.request_id) {
            let _ = pending.response_tx.send(Ok(frame.full_text));
        }
    }

    /// Resolves a `webgpu-error` frame's matching pending request with an
    /// error and removes it from the pending map.
    pub fn handle_error(&self, frame: WebGpuErrorFrame) {
        if let Some((_, pending)) = self.inner.pending.remove(&frame.request_id) {
            let _ = pending
                .response_tx
                .send(Err(ParishError::Inference(frame.message)));
        }
    }

    /// Cancels every in-flight request, replying with `message` to each.
    /// Called when the WebSocket closes so callers don't block forever
    /// waiting on a browser that has gone away.
    pub fn cancel_all(&self, message: &str) {
        let ids: Vec<u64> = self.inner.pending.iter().map(|e| *e.key()).collect();
        for id in ids {
            if let Some((_, pending)) = self.inner.pending.remove(&id) {
                let _ = pending
                    .response_tx
                    .send(Err(ParishError::Inference(message.to_string())));
            }
        }
    }

    /// Number of in-flight requests, for diagnostics.
    pub fn in_flight(&self) -> usize {
        self.inner.pending.len()
    }
}

impl WebGpuTransport for WebGpuBridge {
    fn submit(&self, req: WebGpuRequest) -> oneshot::Receiver<Result<String, ParishError>> {
        let request_id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let (response_tx, response_rx) = oneshot::channel();

        // Derive the wire-level model name — fall back to the bridge's
        // default if the caller didn't pin one (e.g. base GameConfig with
        // model_name = "" for the auto-detect path).
        let model = if req.model.trim().is_empty() {
            DEFAULT_WEBGPU_MODEL.to_string()
        } else {
            req.model.clone()
        };

        self.inner.pending.insert(
            request_id,
            PendingRequest {
                response_tx,
                token_tx: req.token_tx.clone(),
            },
        );

        let frame = WebGpuGenerateFrame {
            request_id,
            model,
            prompt: req.prompt,
            system: req.system,
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            streaming: req.token_tx.is_some(),
            json_mode: req.json_mode,
        };

        // Serialise manually so we can use `EventBus::send` directly and
        // observe the receiver count — `emit` drops that signal.
        let payload = match serde_json::to_value(&frame) {
            Ok(v) => v,
            Err(e) => {
                self.reject_pending(
                    request_id,
                    format!("failed to serialise webgpu-generate frame: {e}"),
                );
                return response_rx;
            }
        };
        let subscriber_count = self.inner.event_bus.send(ServerEvent {
            event: "webgpu-generate".to_string(),
            payload,
        });

        // Fast-fail when no WebSocket is currently subscribed — otherwise
        // the pending entry sits forever (no `webgpu-end` frame can ever
        // arrive) until the next `cancel_all` runs. This turns a silent
        // hang into an immediate, actionable error.
        if subscriber_count == 0 {
            self.reject_pending(
                request_id,
                "WebGPU bridge has no connected browser; open the web UI in a WebGPU-capable \
                 tab or switch to a different provider."
                    .to_string(),
            );
        }

        response_rx
    }
}

impl WebGpuBridge {
    /// Resolves the pending entry for `request_id` with `ParishError::Inference(message)`
    /// if it still exists. Used by the fast-fail paths in `submit`.
    fn reject_pending(&self, request_id: u64, message: String) {
        if let Some((_, pending)) = self.inner.pending.remove(&request_id) {
            let _ = pending
                .response_tx
                .send(Err(ParishError::Inference(message)));
        }
    }
}

/// Post-build attachment helper: walks the session's base/cloud clients
/// and swaps any `WebGpuClient::unavailable` handles for ones backed by
/// this session's bridge.
///
/// `build_session_client` and `build_session_cloud_client` run before the
/// [`AppState`] (and therefore the bridge) exist, so this has to run as a
/// follow-up step on the constructed state.
///
/// Returns a clone of the updated base client so callers can feed it
/// straight into `init_inference_queue` without re-locking.
pub async fn attach_webgpu_bridge_to_session_clients(
    app_state: &crate::state::AppState,
) -> Option<parish_core::inference::AnyClient> {
    let transport: Arc<dyn WebGpuTransport> = Arc::new(app_state.webgpu_bridge.clone());

    // Base client.
    let updated = {
        let mut guard = app_state.client.lock().await;
        let updated = guard.take().map(|c| c.with_webgpu_transport(&transport));
        *guard = updated.clone();
        updated
    };

    // Cloud client (rare with WebGPU but handle consistently).
    {
        let mut guard = app_state.cloud_client.lock().await;
        let updated_cloud = guard.take().map(|c| c.with_webgpu_transport(&transport));
        *guard = updated_cloud;
    }

    updated
}

/// Opaque wrapper around an inbound text frame from the WebSocket so the
/// router doesn't have to know about every WebGPU frame variant.
///
/// Returns `Ok(true)` if the frame was a recognised `webgpu-*` event and
/// has been routed; `Ok(false)` if it was something else (the WS handler
/// can ignore it); `Err` if it looked like a WebGPU frame but its payload
/// failed to parse.
pub fn route_inbound(bridge: &WebGpuBridge, raw: &str) -> Result<bool, serde_json::Error> {
    #[derive(Deserialize)]
    struct Envelope<'a> {
        #[serde(borrow)]
        event: &'a str,
        payload: serde_json::Value,
    }

    let env: Envelope = match serde_json::from_str(raw) {
        Ok(env) => env,
        // Not JSON — treat as "not for us". The WS handler currently
        // ignores non-JSON client messages anyway.
        Err(_) => return Ok(false),
    };

    match env.event {
        "webgpu-token" => {
            let frame: WebGpuTokenFrame = serde_json::from_value(env.payload)?;
            bridge.handle_token(frame);
            Ok(true)
        }
        "webgpu-end" => {
            let frame: WebGpuEndFrame = serde_json::from_value(env.payload)?;
            bridge.handle_end(frame);
            Ok(true)
        }
        "webgpu-error" => {
            let frame: WebGpuErrorFrame = serde_json::from_value(env.payload)?;
            bridge.handle_error(frame);
            Ok(true)
        }
        _ => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drains every event currently in the broadcast channel into a vec.
    async fn drain_events(
        rx: &mut tokio::sync::broadcast::Receiver<ServerEvent>,
    ) -> Vec<ServerEvent> {
        let mut out = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            out.push(ev);
        }
        out
    }

    #[tokio::test]
    async fn submit_emits_webgpu_generate_event() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();
        let bridge = WebGpuBridge::new(bus);

        let _resp_rx = bridge.submit(WebGpuRequest {
            model: "onnx-community/gemma-4-E2B-it-ONNX".into(),
            prompt: "hello".into(),
            system: Some("you are a parish elder".into()),
            max_tokens: Some(64),
            temperature: Some(0.7),
            json_mode: false,
            token_tx: None,
        });

        // Yield so the broadcast send is observable by the receiver.
        tokio::task::yield_now().await;
        let events = drain_events(&mut rx).await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "webgpu-generate");
        assert_eq!(
            events[0].payload["model"],
            "onnx-community/gemma-4-E2B-it-ONNX"
        );
        assert_eq!(events[0].payload["prompt"], "hello");
        assert_eq!(events[0].payload["streaming"], false);
        assert_eq!(events[0].payload["request_id"], 1);
    }

    #[tokio::test]
    async fn empty_model_falls_back_to_default() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();
        let bridge = WebGpuBridge::new(bus);

        let _resp_rx = bridge.submit(WebGpuRequest {
            model: String::new(),
            prompt: "hi".into(),
            system: None,
            max_tokens: None,
            temperature: None,
            json_mode: false,
            token_tx: None,
        });
        tokio::task::yield_now().await;
        let ev = rx.try_recv().unwrap();
        assert_eq!(ev.payload["model"], DEFAULT_WEBGPU_MODEL);
    }

    #[tokio::test]
    async fn end_frame_resolves_pending_response() {
        let bus = EventBus::new(16);
        // Keep a subscriber alive so `submit` doesn't fast-fail on the
        // zero-receiver path — that path has its own dedicated test.
        let _keep_alive = bus.subscribe();
        let bridge = WebGpuBridge::new(bus);

        let resp_rx = bridge.submit(WebGpuRequest {
            model: "m".into(),
            prompt: "p".into(),
            system: None,
            max_tokens: None,
            temperature: None,
            json_mode: false,
            token_tx: None,
        });

        bridge.handle_end(WebGpuEndFrame {
            request_id: 1,
            full_text: "Sure now and that's the truth.".into(),
        });

        let result = resp_rx.await.unwrap().unwrap();
        assert_eq!(result, "Sure now and that's the truth.");
        assert_eq!(bridge.in_flight(), 0);
    }

    #[tokio::test]
    async fn token_frames_forward_into_stream_channel() {
        let bus = EventBus::new(16);
        let _keep_alive = bus.subscribe();
        let bridge = WebGpuBridge::new(bus);
        let (tok_tx, mut tok_rx) = mpsc::unbounded_channel();

        let resp_rx = bridge.submit(WebGpuRequest {
            model: "m".into(),
            prompt: "p".into(),
            system: None,
            max_tokens: None,
            temperature: None,
            json_mode: false,
            token_tx: Some(tok_tx),
        });

        bridge.handle_token(WebGpuTokenFrame {
            request_id: 1,
            delta: "Sure ".into(),
        });
        bridge.handle_token(WebGpuTokenFrame {
            request_id: 1,
            delta: "now.".into(),
        });
        bridge.handle_end(WebGpuEndFrame {
            request_id: 1,
            full_text: "Sure now.".into(),
        });

        assert_eq!(tok_rx.recv().await.unwrap(), "Sure ");
        assert_eq!(tok_rx.recv().await.unwrap(), "now.");
        let result = resp_rx.await.unwrap().unwrap();
        assert_eq!(result, "Sure now.");
    }

    #[tokio::test]
    async fn error_frame_rejects_pending_response() {
        let bus = EventBus::new(16);
        let _keep_alive = bus.subscribe();
        let bridge = WebGpuBridge::new(bus);

        let resp_rx = bridge.submit(WebGpuRequest {
            model: "m".into(),
            prompt: "p".into(),
            system: None,
            max_tokens: None,
            temperature: None,
            json_mode: false,
            token_tx: None,
        });
        bridge.handle_error(WebGpuErrorFrame {
            request_id: 1,
            message: "WebGPU adapter not available".into(),
        });

        let err = resp_rx.await.unwrap().unwrap_err();
        assert!(err.to_string().contains("WebGPU adapter"));
        assert_eq!(bridge.in_flight(), 0);
    }

    #[tokio::test]
    async fn cancel_all_rejects_every_pending_request() {
        let bus = EventBus::new(16);
        let _keep_alive = bus.subscribe();
        let bridge = WebGpuBridge::new(bus);

        let r1 = bridge.submit(WebGpuRequest {
            model: "m".into(),
            prompt: "a".into(),
            system: None,
            max_tokens: None,
            temperature: None,
            json_mode: false,
            token_tx: None,
        });
        let r2 = bridge.submit(WebGpuRequest {
            model: "m".into(),
            prompt: "b".into(),
            system: None,
            max_tokens: None,
            temperature: None,
            json_mode: false,
            token_tx: None,
        });

        bridge.cancel_all("browser disconnected");
        assert!(
            r1.await
                .unwrap()
                .unwrap_err()
                .to_string()
                .contains("disconnected")
        );
        assert!(
            r2.await
                .unwrap()
                .unwrap_err()
                .to_string()
                .contains("disconnected")
        );
        assert_eq!(bridge.in_flight(), 0);
    }

    #[tokio::test]
    async fn route_inbound_dispatches_known_events() {
        let bus = EventBus::new(16);
        let _keep_alive = bus.subscribe();
        let bridge = WebGpuBridge::new(bus);
        let resp_rx = bridge.submit(WebGpuRequest {
            model: "m".into(),
            prompt: "p".into(),
            system: None,
            max_tokens: None,
            temperature: None,
            json_mode: false,
            token_tx: None,
        });

        // Simulate a JSON envelope arriving on the WebSocket.
        let raw = r#"{"event":"webgpu-end","payload":{"request_id":1,"full_text":"hi"}}"#;
        assert!(route_inbound(&bridge, raw).unwrap());
        let result = resp_rx.await.unwrap().unwrap();
        assert_eq!(result, "hi");
    }

    #[tokio::test]
    async fn route_inbound_ignores_unknown_events() {
        let bus = EventBus::new(16);
        let bridge = WebGpuBridge::new(bus);
        // A non-WebGPU frame, e.g. a regular client ping.
        let raw = r#"{"event":"ping","payload":{}}"#;
        assert!(!route_inbound(&bridge, raw).unwrap());
    }

    #[tokio::test]
    async fn route_inbound_returns_false_for_non_json() {
        let bus = EventBus::new(16);
        let bridge = WebGpuBridge::new(bus);
        assert!(!route_inbound(&bridge, "not json at all").unwrap());
    }

    /// Regression guard for the "no subscriber" stall: if `submit` fires
    /// while no WebSocket is listening the bridge must reject the pending
    /// request immediately instead of leaving it to time out elsewhere.
    #[tokio::test]
    async fn submit_fast_fails_when_no_subscriber_is_connected() {
        let bus = EventBus::new(16);
        // Deliberately do NOT call `subscribe` — the bridge should detect
        // the zero-receiver state and resolve the oneshot with an error.
        let bridge = WebGpuBridge::new(bus);

        let rx = bridge.submit(WebGpuRequest {
            model: "m".into(),
            prompt: "p".into(),
            system: None,
            max_tokens: None,
            temperature: None,
            json_mode: false,
            token_tx: None,
        });
        let err = rx.await.unwrap().unwrap_err();
        assert!(
            err.to_string().contains("no connected browser"),
            "got: {err}"
        );
        assert_eq!(bridge.in_flight(), 0);
    }

    /// When at least one subscriber is listening, `submit` should stay in
    /// its normal path (pending entry remains, no pre-emptive error).
    #[tokio::test]
    async fn submit_leaves_pending_entry_when_subscriber_present() {
        let bus = EventBus::new(16);
        let _rx_keep_alive = bus.subscribe();
        let bridge = WebGpuBridge::new(bus);

        let _resp_rx = bridge.submit(WebGpuRequest {
            model: "m".into(),
            prompt: "p".into(),
            system: None,
            max_tokens: None,
            temperature: None,
            json_mode: false,
            token_tx: None,
        });
        assert_eq!(bridge.in_flight(), 1);
    }
}
