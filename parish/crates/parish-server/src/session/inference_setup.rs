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
    *app_state.inference.inference_queue.lock().await = Some(queue);
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
    // Acquire the sidecar before Database::open can create/migrate SQLite.
    let candidate_lock = parish_core::persistence::SaveFileLock::try_acquire(&save_path)
        .ok_or_else(|| format!("Could not lock new save file {}", save_path.display()))?;
    let save_path_clone = save_path.clone();

    let branch_id = tokio::task::spawn_blocking(move || -> Result<i64, String> {
        let db = Database::open(&save_path_clone).map_err(|e| e.to_string())?;
        // `Database::open` already auto-creates the "main" branch, so reuse it
        // rather than calling `create_branch("main", ...)` again — the latter
        // trips a `UNIQUE constraint failed: branches.name` error, which left
        // the save fields unset and `/api/save-state` returning all-null on a
        // freshly-created session (#9). Mirrors `new_save_file`.
        let branch = db
            .find_branch("main")
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "main branch missing after Database::open".to_string())?;
        db.save_snapshot(branch.id, &snapshot)
            .map_err(|e| e.to_string())?;
        Ok(branch.id)
    })
    .await
    .map_err(|e| e.to_string())??;

    let prepared_binding = app_state
        .session_store
        .prepare_active_save(&app_state.session_id, &save_path)
        .map_err(|error| error.to_string())?;
    parish_core::persistence::write_active_save_identity(
        session_saves,
        &save_path,
        branch_id,
        "main",
    )
    .map_err(|error| error.to_string())?;

    // Marker is the commit record; publication below cannot fail.
    prepared_binding.commit();
    *app_state.save_lock.lock().await = Some(candidate_lock);
    app_state
        .save_identity
        .replace(save_path.clone(), branch_id, "main".to_string())
        .await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #9 regression: `Database::open` auto-creates the "main" branch, so
    /// `init_session_save` must *reuse* it. The earlier `create_branch("main")`
    /// tripped `UNIQUE constraint failed: branches.name`, returned `Err`, and
    /// left `save_path` / `current_branch_id` / `current_branch_name` unset —
    /// which surfaced as an all-null `GET /api/save-state` on every freshly
    /// created server session.
    #[tokio::test]
    async fn init_session_save_populates_branch_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let session_saves = tmp.path().join("test-session");
        std::fs::create_dir_all(&session_saves).unwrap();
        let mut state = crate::routes::tests::test_app_state();
        let state_parts =
            Arc::get_mut(&mut state).expect("fresh test state must be uniquely owned");
        state_parts.saves_dir = session_saves.clone();
        state_parts.session_store = Arc::new(crate::session_store_impl::DbSessionStore::new(
            tmp.path().to_path_buf(),
        ));

        init_session_save(&state, &session_saves)
            .await
            .expect("init_session_save must succeed on a fresh save (no UNIQUE-branch panic)");

        assert!(
            state.save_identity.save_path.lock().await.is_some(),
            "save_path must be set after init_session_save"
        );
        assert_eq!(
            *state.save_identity.current_branch_id.lock().await,
            Some(1),
            "the auto-created main branch has id 1"
        );
        assert_eq!(
            state
                .save_identity
                .current_branch_name
                .lock()
                .await
                .as_deref(),
            Some("main"),
            "branch name must be main"
        );
    }
}
