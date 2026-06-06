//! Inference client construction and per-session save initialisation.
//!
//! Owns: `build_session_client`, `build_session_cloud_client`,
//! `init_inference_queue`, `init_session_save`.

use std::path::Path;
use std::sync::Arc;

use parish_core::inference::{
    AnyClient, InferenceQueue, InferenceWorkerConfig, spawn_inference_worker,
};
use parish_core::ipc::GameConfig;

use crate::state::AppState;

use super::GlobalState;

// ── Inference client construction ─────────────────────────────────────────────

pub(super) fn build_session_client(global: &GlobalState) -> (Option<AnyClient>, GameConfig) {
    let config = global.template_config.clone();
    let client = if config.provider_name == "simulator" {
        Some(AnyClient::simulator())
    } else if config.model_name.is_empty() && config.provider_name != "ollama" {
        None
    } else {
        let provider_enum = parish_core::config::Provider::from_str_loose(&config.provider_name)
            .unwrap_or_default();
        Some(parish_core::inference::build_client(
            &provider_enum,
            &config.base_url,
            config.api_key.as_deref(),
            &global.inference_config, // (#417) use TOML-configured timeouts
        ))
    };
    (client, config)
}

pub(super) fn build_session_cloud_client(global: &GlobalState) -> Option<AnyClient> {
    let config = &global.template_config;
    config.cloud_api_key.as_deref().map(|key| {
        let provider_enum = config
            .cloud_provider_name
            .as_deref()
            .and_then(|p| parish_core::config::Provider::from_str_loose(p).ok())
            .unwrap_or_else(|| {
                parish_core::config::Provider::from_id("openrouter").unwrap_or_default()
            });
        parish_core::inference::build_client(
            &provider_enum,
            config
                .cloud_base_url
                .as_deref()
                .unwrap_or("https://openrouter.ai/api"),
            Some(key),
            &global.inference_config, // (#417) use TOML-configured timeouts
        )
    })
}

// ── Inference queue initialisation ───────────────────────────────────────────

pub(super) async fn init_inference_queue(app_state: &Arc<AppState>, client: AnyClient) {
    let (interactive_tx, interactive_rx) = tokio::sync::mpsc::channel(16);
    let (background_tx, background_rx) = tokio::sync::mpsc::channel(32);
    let (batch_tx, batch_rx) = tokio::sync::mpsc::channel(64);
    let provider =
        parish_core::config::Provider::from_str_loose(&app_state.config.lock().await.provider_name)
            .unwrap_or_default();
    let worker = spawn_inference_worker(
        client,
        InferenceWorkerConfig {
            interactive_rx,
            background_rx,
            batch_rx,
            log: app_state.inference_log.clone(),
            file_log: app_state.inference_file_log.clone(),
            provider,
            timeout_config: app_state.inference_config.clone(),
        },
    );
    let queue = InferenceQueue::new(interactive_tx, background_tx, batch_tx);
    *app_state.inference_queue.lock().await = Some(queue);
    *app_state.worker_handle.lock().await = Some(worker);
}

// ── Initial save ──────────────────────────────────────────────────────────────

/// Saves the initial world snapshot into `saves/<session_id>/parish_001.db`.
pub(super) async fn init_session_save(
    app_state: &Arc<AppState>,
    session_saves: &Path,
) -> Result<(), String> {
    use parish_core::persistence::Database;
    use parish_core::persistence::picker::new_save_path;
    use parish_core::persistence::snapshot::GameSnapshot;

    let snapshot = {
        let world = app_state.world.lock().await;
        let npc_manager = app_state.npc_manager.lock().await;
        GameSnapshot::capture(&world, &npc_manager)
    };

    let save_path = new_save_path(session_saves);
    let save_path_clone = save_path.clone();

    let branch_id = tokio::task::spawn_blocking(move || -> Result<i64, String> {
        let db = Database::open(&save_path_clone).map_err(|e| e.to_string())?;
        let branch_id = db.create_branch("main", None).map_err(|e| e.to_string())?;
        db.save_snapshot(branch_id, &snapshot)
            .map_err(|e| e.to_string())?;
        Ok(branch_id)
    })
    .await
    .map_err(|e| e.to_string())??;

    // Advisory lock on the freshly-initialised save file so peer
    // instances don't write to it concurrently (#425). For a just-created
    // save we expect the lock to always succeed, but we stay defensive:
    // warn if the lock fails rather than silently proceeding.
    let locked = parish_core::persistence::SaveFileLock::try_acquire(&save_path);
    if locked.is_none() {
        tracing::warn!(
            path = %save_path.display(),
            "SaveFileLock::try_acquire returned None on init_session_save — new save file unexpectedly locked",
        );
    }
    *app_state.save_lock.lock().await = locked;
    *app_state.save_path.lock().await = Some(save_path);
    *app_state.current_branch_id.lock().await = Some(branch_id);
    *app_state.current_branch_name.lock().await = Some("main".to_string());

    Ok(())
}
