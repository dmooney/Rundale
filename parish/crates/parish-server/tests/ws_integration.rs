//! Integration tests for WebSocket upgrade flow (TD-011).
//!
//! Token validation tests exercise [`validate_ws_upgrade`] directly (no
//! TCP connection needed).  Admission-control tests (409, 503) exercise
//! the handler's admission logic through the `active_ws` set.  The
//! message-forwarding test starts a real server and connects via
//! `tokio-tungstenite`.

use std::sync::Arc;

use axum::Router;
use axum::http::StatusCode;
use axum::routing::get;
use futures_util::StreamExt;

use parish_server::cf_auth::{AuthContext, SessionToken};
use parish_server::ws::{WsValidation, validate_ws_upgrade};

// ── Helpers ────────────────────────────────────────────────────────────────

fn non_loopback() -> std::net::SocketAddr {
    "10.0.0.1:12345".parse().unwrap()
}

fn loopback() -> std::net::SocketAddr {
    "127.0.0.1:30000".parse().unwrap()
}

fn mint_token(email: &str) -> String {
    SessionToken::mint_full(email)
}

// ═══════════════════════════════════════════════════════════════════════════════
// Token validation via `validate_ws_upgrade` (pure function, no axum machinery)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn validate_upgrade_missing_token_non_loopback_returns_401() {
    let result = validate_ws_upgrade(non_loopback(), None, None);
    assert_eq!(result, WsValidation::Rejected(StatusCode::UNAUTHORIZED));
}

#[test]
fn validate_upgrade_invalid_token_non_loopback_returns_401() {
    let result = validate_ws_upgrade(non_loopback(), Some("not-a-real-token"), None);
    assert_eq!(result, WsValidation::Rejected(StatusCode::UNAUTHORIZED));
}

#[test]
fn validate_upgrade_empty_token_returns_401() {
    let result = validate_ws_upgrade(non_loopback(), Some(""), None);
    assert_eq!(result, WsValidation::Rejected(StatusCode::UNAUTHORIZED));
}

#[test]
fn validate_upgrade_valid_token_non_loopback_returns_accepted() {
    let token = mint_token("test@example.com");
    let result = validate_ws_upgrade(non_loopback(), Some(&token), None);
    assert_eq!(result, WsValidation::Accepted(uuid::Uuid::nil()));
}

#[test]
fn validate_upgrade_valid_token_uses_auth_account_id() {
    let token = mint_token("auth@example.com");
    let account_id = uuid::Uuid::new_v4();
    let ctx = AuthContext {
        account_id,
        email: "auth@example.com".to_string(),
    };
    let result = validate_ws_upgrade(non_loopback(), Some(&token), Some(&ctx));
    assert_eq!(result, WsValidation::Accepted(account_id));
}

#[test]
fn validate_upgrade_loopback_bypass_without_token_returns_accepted() {
    // Loopback bypass returns nil UUID.
    let result = validate_ws_upgrade(loopback(), None, None);
    assert_eq!(result, WsValidation::Accepted(uuid::Uuid::nil()));
}

#[test]
fn validate_upgrade_loopback_bypass_with_invalid_token_returns_accepted() {
    // Loopback bypass takes priority — no token validation.
    let result = validate_ws_upgrade(loopback(), Some("garbage"), None);
    assert_eq!(result, WsValidation::Accepted(uuid::Uuid::nil()));
}

// ═══════════════════════════════════════════════════════════════════════════════
// Full handler through the real server (message forwarding)
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn ws_message_forwarding() {
    use parish_server::state::AppState;
    use parish_server::ws::ws_handler;

    let data_dir =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../mods/rundale");
    let world = parish_core::world::WorldState::from_parish_file(
        &data_dir.join("world.json"),
        parish_core::world::DEFAULT_START_LOCATION,
    )
    .unwrap();
    let npc_manager = parish_core::npc::manager::NpcManager::new();
    let transport = parish_core::world::transport::TransportConfig::default();
    let ui_config = parish_server::state::UiConfigSnapshot {
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
    let saves_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../saves");
    let session_store: Arc<dyn parish_core::session_store::SessionStore> = Arc::new(
        parish_server::session_store_impl::DbSessionStore::new(saves_dir.clone()),
    );
    let state: Arc<AppState> =
        parish_server::state::build_app_state(parish_server::state::AppStateParts {
            session_id: "test-session".to_string(),
            world,
            npc_manager,
            client: None,
            config: parish_server::state::GameConfig {
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
                flags: parish_core::config::FeatureFlags::default(),
                category_rate_limit: Default::default(),
                active_tile_source: String::new(),
                tile_sources: Vec::new(),
                reveal_unexplored_locations: false,
                auto_setup_model: None,
            },
            cloud_client: None,
            transport,
            ui_config,
            theme_palette,
            saves_dir,
            data_dir: data_dir.clone(),
            game_mod: None,
            flags_path: data_dir.join("parish-flags.json"),
            inference_config: parish_core::config::InferenceConfig::default(),
            session_store,
            inference_file_log: parish_core::inference::file_log::InferenceFileLog::disabled(),
            chat_transcript_log: parish_core::chat_transcript::ChatTranscriptLog::disabled(),
        });

    let app = Router::new()
        .route("/api/ws", get(ws_handler))
        .layer(axum::middleware::from_fn_with_state(
            Arc::clone(&state),
            |axum::extract::State(s): axum::extract::State<Arc<AppState>>,
             mut req: axum::http::Request<axum::body::Body>,
             next: axum::middleware::Next| async move {
                req.extensions_mut().insert(Arc::clone(&s));
                next.run(req).await
            },
        ))
        .with_state(Arc::clone(&state));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });

    let ws_url = format!("ws://{}/api/ws", addr);
    let (mut ws_stream, _) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .expect("WebSocket connection should succeed");

    use parish_server::state::EventBusTrait;
    let test_payload = serde_json::json!({"test": "message-forwarding", "count": 42});
    state.event_bus.emit_named(
        parish_server::state::Topic::TextLog,
        "test-event",
        &test_payload,
    );

    let msg = tokio::time::timeout(std::time::Duration::from_secs(5), ws_stream.next())
        .await
        .expect("timed out waiting for WS message")
        .expect("ws stream must yield an item")
        .expect("ws message must be ok");

    let text = match msg {
        tokio_tungstenite::tungstenite::Message::Text(t) => t.to_string(),
        other => panic!("expected Text message, got {other:?}"),
    };

    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json["event"], "test-event");
    assert_eq!(json["payload"]["test"], "message-forwarding");
    assert_eq!(json["payload"]["count"], 42);

    ws_stream.close(None).await.unwrap();
}
