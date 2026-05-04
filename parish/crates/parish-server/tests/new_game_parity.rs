/// Integration tests for #745 — `do_new_game_inner` error parity.
///
/// Before the fix, a missing or corrupt world file caused `do_new_game_inner`
/// to silently fall back to `WorldState::new()` while returning `Ok(())`.
/// The Tauri backend (`do_new_game`) propagated the same failure as an `Err`.
///
/// After the fix, `POST /api/new-game` returns `500 Internal Server Error`
/// when the data directory contains no loadable world file, matching the
/// Tauri backend's error-propagation behaviour.
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use axum::{Extension, Router};
use tower::ServiceExt as _;

use parish_core::config::InferenceConfig;
use parish_core::npc::manager::NpcManager;
use parish_core::world::transport::TransportConfig;
use parish_core::world::{DEFAULT_START_LOCATION, WorldState};
use parish_server::routes::new_game;
use parish_server::state::{GameConfig, UiConfigSnapshot, build_app_state};

fn default_game_config() -> GameConfig {
    GameConfig {
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
    }
}

fn default_ui_config() -> UiConfigSnapshot {
    UiConfigSnapshot {
        hints_label: "test".to_string(),
        default_accent: "#000".to_string(),
        splash_text: String::new(),
        active_tile_source: String::new(),
        tile_sources: Vec::new(),
        auto_pause_timeout_seconds: 300,
    }
}

/// Build a minimal router for `POST /api/new-game` backed by the given state.
fn new_game_router(state: Arc<parish_server::state::AppState>) -> Router {
    Router::new()
        .route("/api/new-game", post(new_game))
        .layer(Extension(state))
}

// ── #745 error-parity tests ───────────────────────────────────────────────────

/// When the data directory exists but contains no `parish.json` / `world.json`
/// and no game mod is active, `POST /api/new-game` must return `500`.
///
/// Before the fix this returned `200 OK` with a silent empty-world fallback.
#[tokio::test]
async fn new_game_with_missing_world_file_returns_500() {
    // Use a real temporary directory with no game files inside it.
    let tmp = tempfile::tempdir().expect("tempdir");
    let data_dir = tmp.path().to_path_buf();
    let saves_dir = tmp.path().join("saves");
    std::fs::create_dir_all(&saves_dir).ok();

    // Build an AppState that has NO game_mod and a data_dir with no world file.
    // The initial world here is a default one (the AppState constructor needs
    // *something*); `do_new_game_inner` will attempt to reload from data_dir.
    let state = build_app_state(
        "test-session".to_string(),
        WorldState::new(),
        NpcManager::new(),
        None,
        default_game_config(),
        None,
        TransportConfig::default(),
        default_ui_config(),
        parish_core::game_mod::default_theme_palette(),
        saves_dir.clone(),
        data_dir.clone(),
        None, // no game_mod → legacy fallback path is taken
        data_dir.join("parish-flags.json"),
        InferenceConfig::default(),
    );

    let req = Request::builder()
        .method("POST")
        .uri("/api/new-game")
        .body(Body::empty())
        .expect("build request");

    let resp = new_game_router(state)
        .oneshot(req)
        .await
        .expect("router responded");

    assert_eq!(
        resp.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "POST /api/new-game with a corrupt/missing data dir must return 500, not a silent empty world"
    );
}

/// Sanity-check: `DEFAULT_START_LOCATION` wraps the expected numeric value.
///
/// This pins the constant so a refactor that accidentally changes the ID is
/// caught immediately.  The long-term fix is a mod-manifest `start_location`
/// field; once that ships this constant and test can be removed.
#[test]
fn default_start_location_is_15() {
    assert_eq!(
        DEFAULT_START_LOCATION.0, 15,
        "DEFAULT_START_LOCATION must equal LocationId(15) until the mod-manifest \
         configurable start location is implemented"
    );
}
