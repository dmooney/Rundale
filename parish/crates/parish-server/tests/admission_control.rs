/// Integration tests for admission control (#620).
///
/// Validates that the session middleware returns `503 Service Unavailable`
/// with `Retry-After: 30` when the server is at session capacity.
/// Returning visitors with a valid cookie always pass through.
use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::middleware as axum_mw;
use axum::routing::get;
use tower::ServiceExt as _;

use parish_core::config::{FeatureFlags, InferenceConfig};
use parish_core::game_mod::default_theme_palette;
use parish_core::inference::client::OllamaProcess;
use parish_core::world::transport::TransportConfig;
use parish_server::session::{GlobalState, SessionRegistry};
use parish_server::session_store_impl::{SqliteIdentityStore, open_sessions_db};
use parish_server::state::{GameConfig, UiConfigSnapshot};

// ── Helpers ───────────────────────────────────────────────────────────────────

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
        flags: FeatureFlags::default(),
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

/// Build a minimal `GlobalState` with the given session cap.
///
/// Uses real `SessionRegistry::open` on a tempdir (required for the session
/// insertion logic), but sets `max_concurrent_sessions` to `cap` so the
/// middleware can enforce it.  Starts with zero in-memory sessions.
fn make_global_state(tmp: &tempfile::TempDir, cap: Option<usize>) -> Arc<GlobalState> {
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
        ollama_process: tokio::sync::Mutex::new(OllamaProcess::none()),
        tile_cache: parish_core::tile_cache::TileCache::new(
            tmp.path().join("tile-cache"),
            HashMap::new(),
        ),
        max_concurrent_sessions: cap,
    })
}

/// Build a router that uses the legacy session middleware (no tower-sessions
/// dependency) and exposes `GET /ping` so we can drive requests through it.
fn admission_router(global: Arc<GlobalState>) -> Router {
    Router::new()
        .route("/ping", get(|| async { (StatusCode::OK, "pong") }))
        .route_layer(axum_mw::from_fn_with_state(
            global.clone(),
            parish_server::middleware::session_middleware,
        ))
        .with_state(global)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// When the server has no capacity limit, all requests succeed.
#[tokio::test]
async fn no_cap_allows_unlimited_sessions() {
    let tmp = tempfile::tempdir().unwrap();
    let global = make_global_state(&tmp, None);
    let app = admission_router(global);

    for _ in 0..5 {
        let req = Request::builder().uri("/ping").body(Body::empty()).unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_ne!(
            resp.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "unlimited mode must never return 503"
        );
    }
}

/// With cap=2:
/// - First two requests succeed (each creates a new session).
/// - Third request from a brand-new visitor returns 503 + `Retry-After: 30`.
/// - A fourth request from an *existing* session (cookie present) succeeds
///   because returning visitors bypass the capacity check.
#[tokio::test]
async fn at_cap_new_visitor_gets_503_existing_visitor_allowed() {
    let tmp = tempfile::tempdir().unwrap();
    let global = make_global_state(&tmp, Some(2));
    let app = admission_router(Arc::clone(&global));

    // Helper: extract the parish_sid cookie from a response.
    fn extract_cookie(resp: &axum::response::Response) -> Option<String> {
        for val in resp.headers().get_all(header::SET_COOKIE) {
            if let Ok(s) = val.to_str()
                && let Some(after) = s.strip_prefix("parish_sid=")
            {
                let id = after.split(';').next().unwrap_or("").to_string();
                return Some(id);
            }
        }
        None
    }

    // Request 1 — new visitor, should succeed and set cookie.
    let resp1 = app
        .clone()
        .oneshot(Request::builder().uri("/ping").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_ne!(
        resp1.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "first request must not be 503"
    );
    let cookie1 = extract_cookie(&resp1).expect("first request must set a session cookie");

    // Request 2 — another new visitor, should succeed and set cookie.
    let resp2 = app
        .clone()
        .oneshot(Request::builder().uri("/ping").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_ne!(
        resp2.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "second request must not be 503"
    );
    let cookie2 = extract_cookie(&resp2).expect("second request must set a session cookie");
    // Two distinct sessions.
    assert_ne!(cookie1, cookie2);

    // At this point we have 2 in-memory sessions — the cap is reached.
    assert_eq!(global.sessions.active_count(), 2);

    // Request 3 — a third brand-new visitor: must get 503 with Retry-After.
    let resp3 = app
        .clone()
        .oneshot(Request::builder().uri("/ping").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(
        resp3.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "third new visitor must get 503 when cap=2"
    );
    let retry_after = resp3
        .headers()
        .get(header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(retry_after, "30", "Retry-After header must be '30'");

    // rejection_count must have been incremented.
    assert_eq!(
        global
            .sessions
            .rejection_count
            .load(std::sync::atomic::Ordering::Relaxed),
        1,
        "rejection_count must be 1 after one rejection"
    );

    // Request 4 — returning visitor with cookie1: must succeed even at capacity.
    let resp4 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/ping")
                .header(header::COOKIE, format!("parish_sid={cookie1}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(
        resp4.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "returning visitor must not be rejected even when server is at capacity"
    );
}
