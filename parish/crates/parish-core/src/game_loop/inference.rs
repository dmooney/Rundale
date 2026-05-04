//! Shared inference-rebuild helper (#696).
//!
//! Extracts the common "abort old worker, build new client, spawn new worker,
//! install new queue" logic that was previously duplicated across
//! `parish-server/src/routes.rs` and `parish-tauri/src/commands.rs`.
//!
//! # Usage
//!
//! Each runtime calls [`rebuild_inference_worker`] to handle the mechanical
//! worker lifecycle, then handles the backend-specific side effects itself:
//!
//! - **`parish-server`**: additionally updates the `inference_client` trait
//!   slot and emits a URL warning via the event bus.
//! - **`parish-tauri`**: emits a URL warning via `app.emit`.
//! - **`parish-cli`**: continues to use its own inline implementation (the
//!   headless `App` struct is not yet on `Arc<Mutex<T>>`; deferred to a future
//!   slice — see module-level comment in `game_loop/mod.rs`).
//!
//! # Architecture gate
//!
//! This module is backend-agnostic — it imports only `parish-inference` types
//! and `InferenceConfig`.  It must not import `axum`, `tauri`, or any crate
//! in `FORBIDDEN_FOR_BACKEND_AGNOSTIC`.

use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::config::InferenceConfig;
use crate::inference::{
    AnyClient, InferenceLog, InferenceQueue, build_client, spawn_inference_worker,
};

/// The three AppState mutex slots that [`rebuild_inference_worker`] needs.
///
/// Grouping them into a single struct keeps the function signature within
/// Clippy's `too-many-arguments` limit (≤ 7).
pub struct InferenceSlots<'a> {
    /// `AppState.client` — updated to the new `AnyClient` (skipped for simulator).
    pub client: &'a Mutex<Option<AnyClient>>,
    /// `AppState.worker_handle` — old task aborted, new task stored.
    pub worker_handle: &'a Mutex<Option<JoinHandle<()>>>,
    /// `AppState.inference_queue` — replaced with the new queue.
    pub inference_queue: &'a Mutex<Option<InferenceQueue>>,
}

/// Builds a fresh inference client and worker, aborting the previous worker,
/// and atomically installs both into the caller's mutex slots.
///
/// Returns `(new_client, url_warning)`.
///
/// - `new_client` is the freshly built `AnyClient` so callers can update any
///   additional slots (e.g. the server's trait-erased `inference_client` slot).
/// - `url_warning` is `Some(message)` when the base URL looks malformed
///   (doesn't start with `http://` or `https://`).  Callers are responsible
///   for surfacing this to the player via their runtime's emit path.
///
/// The new worker and queue are installed into [`InferenceSlots`] before this
/// function returns.
///
/// # Lock ordering
///
/// Acquires `slots.client`, then `slots.worker_handle`, then
/// `slots.inference_queue` — callers must not hold any of these locks when
/// calling this function to avoid deadlock.
///
/// # Parameters
///
/// - `provider_name` / `base_url` / `api_key`: values read from `GameConfig`
///   (caller must drop the config lock before calling).
/// - `inference_config`: TOML-configured timeouts; not mutated.
/// - `inference_log`: shared log ring-buffer (cheap `Arc` clone).
/// - `slots`: the three AppState mutex fields used for worker lifecycle.
pub async fn rebuild_inference_worker(
    provider_name: &str,
    base_url: &str,
    api_key: Option<&str>,
    inference_config: &InferenceConfig,
    inference_log: InferenceLog,
    slots: InferenceSlots<'_>,
) -> (AnyClient, Option<String>) {
    // Check URL validity; callers will surface the warning.
    let url_warning = if provider_name != "simulator"
        && !(base_url.starts_with("http://") || base_url.starts_with("https://"))
    {
        Some(format!(
            "Warning: '{}' doesn't look like a valid URL — NPC conversations may fail.",
            base_url
        ))
    } else {
        None
    };

    // Build the new AnyClient and update the raw client slot.
    let any_client = if provider_name == "simulator" {
        AnyClient::simulator()
    } else {
        let provider_enum =
            crate::config::Provider::from_str_loose(provider_name).unwrap_or_default();
        let built = build_client(&provider_enum, base_url, api_key, inference_config);
        {
            let mut guard = slots.client.lock().await;
            *guard = Some(built.clone());
        }
        built
    };

    // Abort the old worker before spawning a replacement, preventing orphaned
    // tasks from accumulating (each holds an HTTP client + channel; bug #224).
    {
        let mut wh = slots.worker_handle.lock().await;
        if let Some(old) = wh.take() {
            old.abort();
        }
    }

    // Spawn fresh channels, worker task, and queue.
    let (interactive_tx, interactive_rx) = tokio::sync::mpsc::channel(16);
    let (background_tx, background_rx) = tokio::sync::mpsc::channel(32);
    let (batch_tx, batch_rx) = tokio::sync::mpsc::channel(64);
    let worker = spawn_inference_worker(
        any_client.clone(),
        interactive_rx,
        background_rx,
        batch_rx,
        inference_log,
        inference_config.clone(),
    );
    let queue = InferenceQueue::new(interactive_tx, background_tx, batch_tx);

    // Install the new queue and worker handle.
    *slots.inference_queue.lock().await = Some(queue);
    *slots.worker_handle.lock().await = Some(worker);

    (any_client, url_warning)
}
