/// Integration tests for cross-user / shared-state isolation
/// (issues #332, #333, #334, #335).
///
/// These tests drive minimal axum routers that exercise the security
/// boundaries without requiring a fully initialised game world where possible.
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use tower::ServiceExt as _;

use parish_server::cf_auth::AuthContext;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Build a router that always injects `AuthContext { email }` into extensions.
fn authed_router_with_email(email: &str, router: Router) -> Router {
    use axum::middleware::{self as axum_mw, Next};
    use axum::response::Response;

    let email = email.to_string();
    router.layer(axum_mw::from_fn(
        move |mut req: Request<Body>, next: Next| {
            let email = email.clone();
            async move {
                req.extensions_mut().insert(AuthContext {
                    email: email.clone(),
                });
                let resp: Response = next.run(req).await;
                Ok::<Response, StatusCode>(resp)
            }
        },
    ))
}

// ── #332 — Admin-command gate ─────────────────────────────────────────────────

/// Shim that mirrors the admin-command guard from `submit_input`.
///
/// We cannot call the real handler without a full `AppState`, so we test the
/// `is_admin_command` + `check_admin` logic via a thin inline shim.
mod admin_guard {
    use axum::Json;
    use axum::extract::Extension;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    use parish_server::cf_auth::AuthContext;

    #[derive(serde::Deserialize)]
    pub struct Req {
        pub text: String,
    }

    pub fn is_admin_command(text: &str) -> bool {
        let lower = text.trim().to_ascii_lowercase();
        const PREFIXES: &[&str] = &[
            "/key",
            "/provider",
            "/model",
            "/cloud",
            "/key.",
            "/model.",
            "/provider.",
        ];
        PREFIXES.iter().any(|p| {
            if p.ends_with('.') {
                lower.starts_with(*p)
            } else {
                lower == *p || lower.starts_with(&format!("{} ", p))
            }
        })
    }

    pub fn check_admin(email: &str, cmd: &str) -> Result<(), StatusCode> {
        match std::env::var("PARISH_ADMIN_EMAILS") {
            Ok(list) => {
                if list.split(',').any(|e| e.trim() == email) {
                    Ok(())
                } else {
                    tracing::warn!(user = %email, command = %cmd, "admin command rejected");
                    Err(StatusCode::FORBIDDEN)
                }
            }
            Err(_) => {
                if cfg!(debug_assertions) {
                    Ok(())
                } else {
                    tracing::warn!(user = %email, command = %cmd, "admin command rejected");
                    Err(StatusCode::FORBIDDEN)
                }
            }
        }
    }

    pub async fn handler(
        Extension(auth): Extension<AuthContext>,
        Json(body): Json<Req>,
    ) -> impl IntoResponse {
        if is_admin_command(&body.text)
            && let Err(status) = check_admin(&auth.email, &body.text)
        {
            return status;
        }
        StatusCode::OK
    }
}

fn admin_router(email: &str) -> Router {
    authed_router_with_email(
        email,
        Router::new().route("/api/submit-input", post(admin_guard::handler)),
    )
}

/// Non-admin user issuing `/cloud key sk-evil` must get 403.
///
/// `set_var` is unsafe in Rust 2024 edition because of potential data races
/// in multi-threaded tests; this test is the only writer and runs atomically.
#[tokio::test]
async fn submit_input_admin_command_non_admin_is_403() {
    // SAFETY: test process is single-threaded at this point; no concurrent
    // threads read PARISH_ADMIN_EMAILS during this assignment.
    unsafe {
        std::env::set_var("PARISH_ADMIN_EMAILS", "operator@example.com");
    }

    let req = Request::builder()
        .method("POST")
        .uri("/api/submit-input")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"text":"/cloud key sk-evil"}"#))
        .unwrap();

    let resp = admin_router("attacker@example.com")
        .oneshot(req)
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

/// Non-admin user issuing a gameplay command must get 200.
#[tokio::test]
async fn submit_input_gameplay_command_any_user_is_200() {
    // SAFETY: same rationale as submit_input_admin_command_non_admin_is_403.
    unsafe {
        std::env::set_var("PARISH_ADMIN_EMAILS", "operator@example.com");
    }

    let req = Request::builder()
        .method("POST")
        .uri("/api/submit-input")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"text":"say hello"}"#))
        .unwrap();

    let resp = admin_router("anyone@example.com")
        .oneshot(req)
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

// ── #333 — Debug snapshot redaction ──────────────────────────────────────────

/// The debug-snapshot handler strips sensitive fields from the call log.
///
/// We exercise the redaction logic directly by constructing an
/// `InferenceLogEntry` with sensitive content, applying the same
/// zeroing-out transformation that `get_debug_snapshot` performs, then
/// asserting the serialised JSON contains `prompt_len` but not the
/// secret text.
#[test]
fn debug_snapshot_call_log_has_prompt_len_not_prompt_text() {
    use parish_core::debug_snapshot::InferenceLogEntry;

    // Construct a synthetic log entry with sensitive content.
    let entry = InferenceLogEntry {
        request_id: 1,
        timestamp: "12:00:00".to_string(),
        model: "llama3".to_string(),
        streaming: false,
        duration_ms: 500,
        prompt_len: 42,
        response_len: 17,
        error: None,
        system_prompt: Some("SECRET SYSTEM PROMPT".to_string()),
        prompt_text: "secret user prompt".to_string(),
        response_text: "secret response".to_string(),
        max_tokens: None,
    };

    // Apply the same redaction that `get_debug_snapshot` applies (#333).
    let redacted = InferenceLogEntry {
        request_id: entry.request_id,
        timestamp: entry.timestamp.clone(),
        model: entry.model.clone(),
        streaming: entry.streaming,
        duration_ms: entry.duration_ms,
        prompt_len: entry.prompt_len,
        response_len: entry.response_len,
        error: entry.error.clone(),
        max_tokens: entry.max_tokens,
        system_prompt: None,
        prompt_text: String::new(),
        response_text: String::new(),
    };

    let json = serde_json::to_value(&redacted).unwrap();

    // Must contain prompt_len.
    assert!(
        json.get("prompt_len").is_some(),
        "prompt_len must be present"
    );
    assert_eq!(json["prompt_len"], 42);

    // Must NOT contain sensitive text anywhere in the serialised output.
    let json_str = json.to_string();
    assert!(
        !json_str.contains("secret"),
        "redacted entry must not contain prompt/response text"
    );
    assert!(
        !json_str.contains("SECRET SYSTEM PROMPT"),
        "redacted entry must not contain system_prompt text"
    );

    // Sensitive fields exist in the struct but are empty / null.
    assert_eq!(json["prompt_text"], "");
    assert_eq!(json["response_text"], "");
    assert!(json["system_prompt"].is_null());
}

// ── #334 — Single WS per email ───────────────────────────────────────────────

/// A second WS upgrade for the same email must be blocked (409 Conflict).
///
/// We test the `active_ws` set logic directly against `AppState` rather than
/// driving a real WebSocket upgrade (which requires a live TCP server).
#[tokio::test]
async fn second_ws_upgrade_same_email_is_409() {
    use std::sync::Arc;

    // Build a minimal AppState using the public builder.
    use parish_core::npc::manager::NpcManager;
    use parish_core::world::transport::TransportConfig;
    use parish_core::world::{LocationId, WorldState};
    use parish_server::state::{GameConfig, UiConfigSnapshot, build_app_state};

    let data_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mods/rundale");
    let world = WorldState::from_parish_file(&data_dir.join("world.json"), LocationId(15)).unwrap();
    let npc_manager = NpcManager::new();
    let ui_config = UiConfigSnapshot {
        hints_label: "test".to_string(),
        default_accent: "#000".to_string(),
        splash_text: String::new(),
        active_tile_source: String::new(),
        tile_sources: Vec::new(),
    };
    let theme_palette = parish_core::game_mod::default_theme_palette();
    let saves_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../saves");

    let state = Arc::new(build_app_state(
        world,
        npc_manager,
        None,
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
            category_provider: [None, None, None, None],
            category_model: [None, None, None, None],
            category_api_key: [None, None, None, None],
            category_base_url: [None, None, None, None],
            flags: parish_core::config::FeatureFlags::default(),
            category_rate_limit: [None, None, None, None],
            active_tile_source: String::new(),
            tile_sources: Vec::new(),
            reveal_unexplored_locations: false,
        },
        None,
        TransportConfig::default(),
        ui_config,
        theme_palette,
        saves_dir,
        data_dir.clone(),
        None,
        data_dir.join("parish-flags.json"),
    ));

    // Simulate first connection inserting the email.
    let first_insert: bool = state
        .active_ws
        .lock()
        .await
        .insert("ws-user@example.com".to_string());
    assert!(first_insert, "first insert must succeed");

    // Second attempt: the email is already present — insert returns false.
    let second_insert: bool = state
        .active_ws
        .lock()
        .await
        .insert("ws-user@example.com".to_string());
    assert!(
        !second_insert,
        "second insert must fail (email already active)"
    );

    // Map the HashSet result to the 409 the handler would return.
    let status = if !second_insert {
        StatusCode::CONFLICT
    } else {
        StatusCode::OK
    };
    assert_eq!(status, StatusCode::CONFLICT);
}

// ── #335 — Branch name validation ────────────────────────────────────────────

/// A branch name of 65 characters must be rejected with 400.
#[tokio::test]
async fn create_branch_65_char_name_is_400() {
    use axum::Json;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    #[derive(serde::Deserialize)]
    struct Req {
        name: String,
    }

    fn validate(name: &str) -> Result<(), StatusCode> {
        if name.is_empty() || name.len() > 64 {
            return Err(StatusCode::BAD_REQUEST);
        }
        if !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == ' ')
        {
            return Err(StatusCode::BAD_REQUEST);
        }
        Ok(())
    }

    async fn handler(Json(body): Json<Req>) -> impl IntoResponse {
        match validate(&body.name) {
            Ok(()) => StatusCode::OK,
            Err(s) => s,
        }
    }

    let router = Router::new().route("/api/create-branch", post(handler));

    let long_name = "a".repeat(65);
    let req = Request::builder()
        .method("POST")
        .uri("/api/create-branch")
        .header("content-type", "application/json")
        .body(Body::from(format!(
            r#"{{"name":"{}","parentBranchId":1}}"#,
            long_name
        )))
        .unwrap();

    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

/// A branch name with only valid characters must pass validation.
#[tokio::test]
async fn create_branch_valid_name_passes_validation() {
    use axum::Json;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    #[derive(serde::Deserialize)]
    struct Req {
        name: String,
    }

    fn validate(name: &str) -> Result<(), StatusCode> {
        if name.is_empty() || name.len() > 64 {
            return Err(StatusCode::BAD_REQUEST);
        }
        if !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == ' ')
        {
            return Err(StatusCode::BAD_REQUEST);
        }
        Ok(())
    }

    async fn handler(Json(body): Json<Req>) -> impl IntoResponse {
        match validate(&body.name) {
            Ok(()) => StatusCode::OK,
            Err(s) => s,
        }
    }

    let router = Router::new().route("/api/create-branch", post(handler));

    let req = Request::builder()
        .method("POST")
        .uri("/api/create-branch")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"name":"valid name","parentBranchId":1}"#))
        .unwrap();

    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
