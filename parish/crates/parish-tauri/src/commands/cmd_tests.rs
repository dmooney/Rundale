//! Shared test helpers for command submodule unit tests.
//!
//! `test_app_state()` builds a minimal [`AppState`] that can be used by any
//! submodule test without spinning up a real Tauri application.

#![cfg(test)]

use std::sync::Arc;

use crate::{
    AppState, ConversationRuntimeState, DEBUG_EVENT_CAPACITY, DemoConfig, GameConfig,
    UiConfigSnapshot,
};
use parish_core::inference::new_inference_log;
use parish_core::npc::manager::NpcManager;
use parish_core::world::transport::TransportConfig;
use parish_core::world::{DEFAULT_START_LOCATION, WorldState};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// Builds a minimal [`AppState`] for unit tests — matches the structure
/// used in `parish-server` tests (`routes::tests::test_app_state`).
pub fn test_app_state() -> Arc<AppState> {
    let data_dir =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../mods/rundale");
    let world =
        WorldState::from_parish_file(&data_dir.join("world.json"), DEFAULT_START_LOCATION).unwrap();
    let npc_manager = NpcManager::new();
    let transport = TransportConfig::default();
    let ui_config = UiConfigSnapshot {
        hints_label: "test".to_string(),
        default_accent: "#000".to_string(),
        splash_text: String::new(),
        active_tile_source: String::new(),
        tile_sources: Vec::new(),
        auto_pause_timeout_seconds: 300,
        app_icon_url: None,
        favicon_url: None,
        map_overlay: None,
        base_mod_required: false,
    };
    let theme_palette = parish_core::game_mod::default_theme_palette();
    let pronunciations = Vec::new();
    let reaction_templates = parish_core::npc::reactions::ReactionTemplates::default();
    let game_config = GameConfig {
        provider_name: String::new(),
        base_url: String::new(),
        api_key: None,
        model_name: String::new(),
        cloud_provider_name: None,
        cloud_model_name: None,
        cloud_api_key: None,
        cloud_base_url: None,
        improv_enabled: false,
        max_follow_up_turns: 2,
        idle_banter_after_secs: 25,
        auto_pause_after_secs: 60,
        category_provider: Default::default(),
        category_model: Default::default(),
        category_api_key: Default::default(),
        category_base_url: Default::default(),
        flags: parish_core::config::FeatureFlags::default(),
        category_rate_limit: Default::default(),
        active_tile_source: String::new(),
        tile_sources: Vec::new(),
        reveal_unexplored_locations: false,
        auto_setup_model: None,
    };
    let saves_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../saves");
    let shutdown_token = CancellationToken::new();
    let session_store: std::sync::Arc<dyn parish_core::session_store::SessionStore> =
        std::sync::Arc::new(parish_core::session_store::DbSessionStore::new(
            saves_dir.clone(),
        ));

    Arc::new(AppState {
        world: Mutex::new(world),
        npc_manager: Mutex::new(npc_manager),
        inference_queue: Mutex::new(None),
        client: Mutex::new(None),
        cloud_client: Mutex::new(None),
        conversation: Mutex::new(ConversationRuntimeState::new()),
        debug_events: Mutex::new(std::collections::VecDeque::with_capacity(
            DEBUG_EVENT_CAPACITY,
        )),
        game_events: Mutex::new(std::collections::VecDeque::with_capacity(
            DEBUG_EVENT_CAPACITY,
        )),
        total_game_events: std::sync::atomic::AtomicUsize::new(0),
        game_mod: None,
        inference_log: new_inference_log(),
        ui_config,
        theme_palette,
        theme_keyframes: Vec::new(),
        static_raw_palette: None,
        inference_failure_messages: Vec::new(),
        idle_messages: Vec::new(),
        pronunciations,
        reaction_templates,
        save_path: Mutex::new(None),
        current_branch_id: Mutex::new(None),
        current_branch_name: Mutex::new(None),
        transport,
        data_dir: data_dir.clone(),
        saves_dir,
        latest_screenshot_path: Mutex::new(None),
        graphical_launch_token: uuid::Uuid::new_v4().to_string(),
        graphical_ready: std::sync::atomic::AtomicBool::new(false),
        graphical_error: std::sync::Mutex::new(None),
        pending_screenshots: Mutex::new(std::collections::HashMap::new()),
        worker_handle: Mutex::new(None),
        editor: std::sync::Mutex::new(parish_core::ipc::editor::EditorSession::default()),
        save_lock: Mutex::new(None),
        runtime_processes: Mutex::new(parish_core::inference::client::RuntimeProcesses::none()),
        inference_config: parish_core::config::InferenceConfig::default(),
        setup_status: std::sync::Mutex::new(crate::SetupStatusSnapshot::default()),
        wizard_in_flight: std::sync::atomic::AtomicBool::new(false),
        language_settings: parish_core::npc::LanguageSettings::english_only(),
        config: Mutex::new(game_config),
        demo_config: DemoConfig::default(),
        shutdown_token,
        sim_cancel: Mutex::new(CancellationToken::new()),
        session_store,
        user_config_dir: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")),
        secret_store: std::sync::Arc::new(parish_core::secret_store::InMemorySecretStore::new()),
        inference_file_log: parish_core::inference::file_log::InferenceFileLog::disabled(),
        chat_transcript_log: parish_core::chat_transcript::ChatTranscriptLog::disabled(),
    })
}

// ── Snapshot and world-state tests ────────────────────────────────────────────

#[tokio::test]
async fn world_state_loads_kilteevan_as_start_location() {
    let state = test_app_state();
    let world = state.world.lock().await;
    let loc_name = world
        .current_location_data()
        .map(|d| d.name.as_str())
        .unwrap_or("unknown");
    // Default start location for Rundale is Kilteevan Village
    assert_eq!(loc_name, "Kilteevan Village");
}

#[tokio::test]
async fn discover_save_files_returns_ok_for_missing_saves_dir() {
    let state = test_app_state();
    let world = state.world.lock().await;
    let nonexistent_dir = std::path::Path::new("/tmp/rundale_test_nonexistent_saves_dir");
    let saves = parish_core::persistence::picker::discover_saves(nonexistent_dir, &world.graph);
    // Missing dir should return empty vec, not panic
    assert!(
        saves.is_empty(),
        "discover_saves should return empty vec for missing directory"
    );
}

#[tokio::test]
async fn get_world_snapshot_inner_returns_start_location() {
    let state = test_app_state();
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let snapshot = super::snapshot::get_world_snapshot_inner(
        &world,
        Some(&npc_manager),
        &state.pronunciations,
    );
    assert!(
        !snapshot.location_name.is_empty(),
        "location name should be populated"
    );
    assert_eq!(snapshot.location_name, "Kilteevan Village");
}
