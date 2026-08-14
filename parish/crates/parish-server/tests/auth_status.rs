//! Integration tests for `GET /api/auth/status` (TD-015).
//!
//! This route provides frontend login-state display and has two code paths:
//!
//! 1. **Extension path** — when the `SessionId` extension is present (injected
//!    by `session_middleware` / `session_middleware_tower`), it is used directly.
//! 2. **Cookie fallback** — when no extension is present, the handler reads
//!    `parish_sid` from the `Cookie` header.
//!
//! In both cases the session id is looked up in the OAuth accounts table; when
//! no linked account is found `logged_in` is `false`.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::routing::get;
use tower::ServiceExt as _;

use parish_core::config::{FeatureFlags, InferenceConfig};
use parish_core::game_mod::default_theme_palette;
use parish_core::inference::client::RuntimeProcesses;
use parish_core::world::transport::TransportConfig;
use parish_server::auth::get_auth_status;
use parish_server::middleware::SessionId;
use parish_server::session::{GlobalState, SessionRegistry};
use parish_server::session_store_impl::{SqliteIdentityStore, open_sessions_db};
use parish_server::state::{GameConfig, UiConfigSnapshot};

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
        inference_profile_override: Default::default(),
        category_inference_profile: Default::default(),
        flags: FeatureFlags::default(),
        category_rate_limit: Default::default(),
        active_tile_source: String::new(),
        tile_sources: Vec::new(),
        reveal_unexplored_locations: false,
        auto_setup_model: None,
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
        app_icon_url: None,
        favicon_url: None,
        map_overlay: None,
        base_mod_required: false,
    }
}

fn make_global_state(tmp: &tempfile::TempDir) -> Arc<GlobalState> {
    let saves_dir = tmp.path().to_path_buf();
    let sessions = SessionRegistry::open(&saves_dir).expect("SessionRegistry::open");
    let identity_conn = open_sessions_db(&saves_dir).expect("open_sessions_db");
    let identity_store: std::sync::Arc<dyn parish_core::identity::IdentityStore> =
        std::sync::Arc::new(SqliteIdentityStore::new(identity_conn));

    Arc::new(GlobalState {
        sessions,
        identity_store,
        oauth_config: None,
        data_dir: saves_dir.clone(),
        world_path: saves_dir.join("world.json"),
        saves_dir,
        game_mod: None,
        pronunciations: vec![],
        ui_config: default_ui_config(),
        theme_palette: default_theme_palette(),
        transport: TransportConfig::default(),
        template_config: default_game_config(),
        inference_config: InferenceConfig::default(),
        runtime_processes: tokio::sync::Mutex::new(RuntimeProcesses::none()),
        tile_cache: parish_core::tile_cache::TileCache::new(
            tmp.path().join("tile-cache"),
            HashMap::new(),
        ),
        idempotency_cache: tokio::sync::Mutex::new(lru::LruCache::new(
            std::num::NonZeroUsize::new(parish_server::session::IDEMPOTENCY_CACHE_CAPACITY)
                .unwrap(),
        )),
        max_concurrent_sessions: None,
    })
}

fn auth_status_router(global: Arc<GlobalState>) -> Router {
    Router::new()
        .route("/api/auth/status", get(get_auth_status))
        .with_state(global)
}

#[tokio::test]
async fn auth_status_no_auth_returns_logged_out() {
    let tmp = tempfile::tempdir().unwrap();
    let global = make_global_state(&tmp);
    let app = auth_status_router(global);

    let req = Request::builder()
        .uri("/api/auth/status")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["oauth_enabled"], false);
    assert_eq!(json["logged_in"], false);
    assert!(json["provider"].is_null());
    assert!(json["display_name"].is_null());
}

#[tokio::test]
async fn auth_status_with_extension_returns_logged_out() {
    let tmp = tempfile::tempdir().unwrap();
    let global = make_global_state(&tmp);
    let app = auth_status_router(global);

    let mut req = Request::builder()
        .uri("/api/auth/status")
        .body(Body::empty())
        .unwrap();
    req.extensions_mut()
        .insert(SessionId("test-session-id".to_string()));

    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["oauth_enabled"], false);
    assert_eq!(json["logged_in"], false);
}

#[tokio::test]
async fn auth_status_with_cookie_returns_logged_out() {
    let tmp = tempfile::tempdir().unwrap();
    let global = make_global_state(&tmp);
    let app = auth_status_router(global);

    let req = Request::builder()
        .uri("/api/auth/status")
        .header(header::COOKIE, "parish_sid=test-session-via-cookie")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["oauth_enabled"], false);
    assert_eq!(json["logged_in"], false);
}
