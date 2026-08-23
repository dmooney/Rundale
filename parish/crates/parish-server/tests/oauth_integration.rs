//! Integration tests for OAuth route handlers (TD-012).
//!
//! Tests all 6 handlers (`login_google`, `callback_google`, `logout`,
//! `login_google_tower`, `callback_google_tower`, `logout_tower`) at the
//! router level using `axum` handlers directly via `tower::ServiceExt`.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::routing::get;
use tower::ServiceExt as _;

use parish_server::auth::{
    callback_google, callback_google_tower, login_google, login_google_tower, logout, logout_tower,
};
use parish_server::session::{GlobalState, OAuthConfig, SessionRegistry};
use parish_server::session_store_impl::{SqliteIdentityStore, open_sessions_db};

// ── Helpers ──────────────────────────────────────────────────────────────────

fn make_global_state(tmp: &tempfile::TempDir, with_oauth: bool) -> Arc<GlobalState> {
    let saves_dir = tmp.path().to_path_buf();
    let sessions = SessionRegistry::open(&saves_dir).expect("SessionRegistry::open");
    let identity_conn = open_sessions_db(&saves_dir).expect("open_sessions_db");
    let identity_store: Arc<dyn parish_core::identity::IdentityStore> =
        Arc::new(SqliteIdentityStore::new(identity_conn));

    Arc::new(GlobalState {
        inference_runtime_v2: None,
        sessions,
        identity_store,
        oauth_config: if with_oauth {
            Some(OAuthConfig {
                client_id: "test-client-id".to_string(),
                client_secret: "test-client-secret".to_string(),
                base_url: "https://rundale.example.com".to_string(),
            })
        } else {
            None
        },
        data_dir: saves_dir.clone(),
        world_path: saves_dir.join("world.json"),
        saves_dir,
        game_mod: None,
        pronunciations: vec![],
        ui_config: parish_server::state::UiConfigSnapshot {
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
        },
        theme_palette: parish_core::game_mod::default_theme_palette(),
        transport: parish_core::world::transport::TransportConfig::default(),
        template_config: default_game_config(),
        inference_config: parish_core::config::InferenceConfig::default(),
        runtime_processes: tokio::sync::Mutex::new(
            parish_core::inference::client::RuntimeProcesses::none(),
        ),
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

fn default_game_config() -> parish_server::state::GameConfig {
    use parish_core::config::FeatureFlags;
    parish_server::state::GameConfig {
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
        ..parish_server::state::GameConfig::default()
    }
}

fn tower_session_router(global: Arc<GlobalState>) -> Router {
    use tower_sessions::cookie::SameSite;
    use tower_sessions::cookie::time::Duration;
    use tower_sessions::{Expiry, MemoryStore, SessionManagerLayer};

    let store = Arc::new(MemoryStore::default());
    let session_layer = SessionManagerLayer::new((*store).clone())
        .with_name("parish_sid")
        .with_secure(false)
        .with_http_only(true)
        .with_same_site(SameSite::Lax)
        .with_path("/".to_string())
        .with_expiry(Expiry::OnInactivity(Duration::days(365)));
    Router::new()
        .route("/auth/login/google", get(login_google_tower))
        .route("/auth/callback/google", get(callback_google_tower))
        .route("/auth/logout", get(logout_tower))
        .layer(session_layer)
        .with_state(global)
}

// ═══════════════════════════════════════════════════════════════════════════════
// Legacy handlers (cookie-based CSRF)
// ═══════════════════════════════════════════════════════════════════════════════

// ── login_google ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn legacy_login_google_no_oauth_returns_404() {
    let tmp = tempfile::tempdir().unwrap();
    let global = make_global_state(&tmp, false);
    let app = Router::new()
        .route("/auth/login/google", get(login_google))
        .with_state(global);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/auth/login/google")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn legacy_login_google_with_oauth_returns_302_and_sets_csrf_cookie() {
    let tmp = tempfile::tempdir().unwrap();
    let global = make_global_state(&tmp, true);
    let app = Router::new()
        .route("/auth/login/google", get(login_google))
        .with_state(global);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/auth/login/google")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SEE_OTHER); // 303

    let set_cookie = resp
        .headers()
        .get(header::SET_COOKIE)
        .expect("login_google must set a CSRF cookie");
    let cookie_str = set_cookie.to_str().unwrap();
    assert!(
        cookie_str.starts_with("parish_oauth_state="),
        "CSRF cookie should be parish_oauth_state=..., got: {cookie_str}"
    );
    assert!(cookie_str.contains("HttpOnly"));

    let location = resp
        .headers()
        .get(header::LOCATION)
        .expect("login_google must redirect");
    let location_str = location.to_str().unwrap();
    assert!(
        location_str.starts_with("https://accounts.google.com/o/oauth2/v2/auth"),
        "redirect should point to Google OAuth, got: {location_str}"
    );
    assert!(location_str.contains("client_id=test-client-id"));
}

// ── callback_google ──────────────────────────────────────────────────────────

#[tokio::test]
async fn legacy_callback_google_missing_code_returns_400() {
    let tmp = tempfile::tempdir().unwrap();
    let global = make_global_state(&tmp, true);
    let app = Router::new()
        .route("/auth/callback/google", get(callback_google))
        .with_state(global);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/auth/callback/google")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn legacy_callback_google_csrf_mismatch_returns_400() {
    let tmp = tempfile::tempdir().unwrap();
    let global = make_global_state(&tmp, true);
    let app = Router::new()
        .route("/auth/callback/google", get(callback_google))
        .with_state(global);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/auth/callback/google?code=abc&state=attacker-state")
                .header(header::COOKIE, "parish_oauth_state=real-state")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn legacy_callback_google_provider_error_redirects_with_error_flag() {
    let tmp = tempfile::tempdir().unwrap();
    let global = make_global_state(&tmp, true);
    let app = Router::new()
        .route("/auth/callback/google", get(callback_google))
        .with_state(global);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/auth/callback/google?error=access_denied")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    let location = resp.headers().get(header::LOCATION).unwrap();
    assert!(
        location.to_str().unwrap().contains("oauth_error=1"),
        "provider error should redirect to /?oauth_error=1"
    );
}

// ── logout ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn legacy_logout_returns_302_and_sets_new_session_cookie() {
    let tmp = tempfile::tempdir().unwrap();
    let global = make_global_state(&tmp, true);
    let app = Router::new()
        .route("/auth/logout", get(logout))
        .with_state(global);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/auth/logout")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);

    let set_cookie = resp
        .headers()
        .get(header::SET_COOKIE)
        .expect("logout must set a new parish_sid cookie");
    let cookie_str = set_cookie.to_str().unwrap();
    assert!(
        cookie_str.starts_with("parish_sid="),
        "logout cookie should be parish_sid=..., got: {cookie_str}"
    );
    assert!(cookie_str.contains("HttpOnly"));
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tower-session handlers (#364)
// ═══════════════════════════════════════════════════════════════════════════════

// ── login_google_tower ────────────────────────────────────────────────────────

#[tokio::test]
async fn tower_login_google_tower_no_oauth_returns_404() {
    let tmp = tempfile::tempdir().unwrap();
    let global = make_global_state(&tmp, false);
    let app = tower_session_router(global);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/auth/login/google")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn tower_login_google_tower_with_oauth_returns_302() {
    let tmp = tempfile::tempdir().unwrap();
    let global = make_global_state(&tmp, true);
    let app = tower_session_router(global);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/auth/login/google")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);

    let location = resp
        .headers()
        .get(header::LOCATION)
        .expect("login_google_tower must redirect");
    let location_str = location.to_str().unwrap();
    assert!(
        location_str.starts_with("https://accounts.google.com/o/oauth2/v2/auth"),
        "redirect should point to Google OAuth, got: {location_str}"
    );
}

// ── callback_google_tower ─────────────────────────────────────────────────────

#[tokio::test]
async fn tower_callback_google_tower_missing_code_returns_400() {
    let tmp = tempfile::tempdir().unwrap();
    let global = make_global_state(&tmp, true);
    let app = tower_session_router(global);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/auth/callback/google")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn tower_callback_google_tower_csrf_mismatch_returns_400() {
    let tmp = tempfile::tempdir().unwrap();
    let global = make_global_state(&tmp, true);
    let app = tower_session_router(global);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/auth/callback/google?code=abc&state=wrong-state")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn tower_callback_google_tower_provider_error_redirects_with_error_flag() {
    let tmp = tempfile::tempdir().unwrap();
    let global = make_global_state(&tmp, true);
    let app = tower_session_router(global);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/auth/callback/google?error=access_denied")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    let location = resp.headers().get(header::LOCATION).unwrap();
    assert!(
        location.to_str().unwrap().contains("oauth_error=1"),
        "provider error should redirect to /?oauth_error=1"
    );
}

// ── logout_tower ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn tower_logout_tower_returns_302() {
    let tmp = tempfile::tempdir().unwrap();
    let global = make_global_state(&tmp, true);
    let app = tower_session_router(global);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/auth/logout")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
}
