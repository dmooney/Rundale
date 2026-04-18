//! Parish web server — serves the Svelte UI in a browser via axum.
//!
//! Provides the same game experience as the Tauri desktop app, but over
//! standard HTTP + WebSocket so it can run in any browser.
//!
//! Each browser visitor gets their own isolated game session, identified by
//! a `parish_sid` cookie.  Sessions are persisted across server restarts via
//! `saves/<session_id>/` directories and `saves/sessions.db`.

pub mod auth;
pub mod editor_routes;
pub mod middleware;
pub mod routes;
pub mod session;
pub mod state;
pub mod ws;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::middleware as axum_mw;
use axum::middleware::Next;
use axum::response::Response;
use axum::routing::{get, post};
use tower_http::services::ServeDir;

use parish_core::game_mod::{GameMod, find_default_mod};
use parish_core::world::transport::TransportConfig;

use parish_core::config::FeatureFlags;
use session::{GlobalState, OAuthConfig, SessionRegistry};
use state::{GameConfig, UiConfigSnapshot};

/// Middleware that enforces Cloudflare Access authentication on non-localhost traffic.
///
/// Requests from loopback addresses (127.0.0.1 / ::1) are always allowed so local
/// development works without a Cloudflare tunnel.  All other requests must carry the
/// `CF-Access-Authenticated-User-Email` header that Cloudflare Access injects after a
/// successful login.  Requests that lack the header are rejected with 401.
async fn cf_access_guard(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if addr.ip().is_loopback() {
        return Ok(next.run(req).await);
    }
    // Allow the health-check endpoint without auth so Railway can probe it.
    if req.uri().path() == "/api/ui-config" {
        return Ok(next.run(req).await);
    }
    if req
        .headers()
        .contains_key("CF-Access-Authenticated-User-Email")
    {
        return Ok(next.run(req).await);
    }
    Err(StatusCode::UNAUTHORIZED)
}

/// Starts the Parish web server on the given port.
pub async fn run_server(port: u16, data_dir: PathBuf, static_dir: PathBuf) -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // ── World path ────────────────────────────────────────────────────────────
    let world_path = {
        let parish = data_dir.join("parish.json");
        let world = data_dir.join("world.json");
        if parish.exists() { parish } else { world }
    };

    // ── LLM client + config (template, cloned per session) ───────────────────
    let (_, mut config) = build_client_and_config();
    let cloud_env = build_cloud_client_from_env();
    config.cloud_provider_name = cloud_env.provider_name;
    config.cloud_model_name = cloud_env.model_name;
    config.cloud_api_key = cloud_env.api_key;
    config.cloud_base_url = cloud_env.base_url;

    // ── Game mod ──────────────────────────────────────────────────────────────
    let game_mod = find_default_mod().and_then(|dir| GameMod::load(&dir).ok());
    let game_title = game_mod
        .as_ref()
        .and_then(|gm| gm.manifest.meta.title.clone())
        .unwrap_or_else(|| "Parish".to_string());

    let commit_sha = std::env::var("RAILWAY_GIT_COMMIT_SHA")
        .or_else(|_| std::env::var("PARISH_COMMIT_SHA"))
        .unwrap_or_else(|_| "unknown".to_string());
    let short_sha: String = commit_sha.chars().take(7).collect();
    let splash_text = format!(
        "{}\nCopyright \u{00A9} 2026 David Mooney. All rights reserved.\nweb-server - {} - build {}",
        game_title,
        chrono::Local::now().format("%Y-%m-%d %H:%M"),
        short_sha,
    );

    let theme_palette = game_mod
        .as_ref()
        .map(|gm| gm.ui.theme.resolved_palette())
        .unwrap_or_else(parish_core::game_mod::default_theme_palette);

    // Load engine config (parish.toml) for the tile-source registry. Missing
    // file or parse errors fall back to baked defaults
    // (OSM + Ireland Historic 6").
    let engine_config = parish_core::config::load_engine_config(None);
    let tile_sources_snapshot =
        parish_core::ipc::TileSourceSnapshot::list_from_map_config(&engine_config.map);
    let active_tile_source = engine_config.map.default_tile_source.clone();
    config.active_tile_source = active_tile_source.clone();
    config.tile_sources = engine_config.map.id_label_pairs();

    let ui_config = if let Some(ref gm) = game_mod {
        UiConfigSnapshot {
            hints_label: gm.ui.sidebar.hints_label.clone(),
            default_accent: theme_palette.accent.clone(),
            splash_text,
            active_tile_source: active_tile_source.clone(),
            tile_sources: tile_sources_snapshot.clone(),
        }
    } else {
        UiConfigSnapshot {
            hints_label: "Language Hints".to_string(),
            default_accent: theme_palette.accent.clone(),
            splash_text,
            active_tile_source,
            tile_sources: tile_sources_snapshot,
        }
    };

    // ── Feature flags ──────────────────────────────────────────────────────
    let flags_path = data_dir.join("parish-flags.json");
    config.flags = FeatureFlags::load_from_file(&flags_path);

    // ── Saves directory ───────────────────────────────────────────────────────
    let saves_dir = parish_core::persistence::picker::ensure_saves_dir();

    // ── Session registry ──────────────────────────────────────────────────────
    let sessions = SessionRegistry::open(&saves_dir)
        .map_err(|e| anyhow::anyhow!("Failed to open sessions.db: {}", e))?;

    // ── Pronunciations (shared, loaded once) ──────────────────────────────────
    let pronunciations = game_mod
        .as_ref()
        .map(|gm| gm.pronunciations.clone())
        .unwrap_or_default();

    // ── Google OAuth config (optional) ────────────────────────────────────────
    let oauth_config = build_oauth_config();
    if oauth_config.is_some() {
        tracing::info!("Google OAuth enabled");
    }

    // ── Global state ──────────────────────────────────────────────────────────
    let global = Arc::new(GlobalState {
        sessions,
        oauth_config,
        data_dir: data_dir.clone(),
        world_path,
        saves_dir,
        game_mod,
        pronunciations,
        ui_config,
        theme_palette,
        transport: TransportConfig::default(),
        template_config: config,
    });

    // ── Session cleanup background task ───────────────────────────────────────
    {
        let g = Arc::clone(&global);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(3600)).await;
                g.sessions.cleanup_stale(Duration::from_secs(86400));
                tracing::debug!("Session cleanup ran");
            }
        });
    }

    // ── Build router ──────────────────────────────────────────────────────────
    let oauth_enabled = global.oauth_config.is_some();

    let mut app = Router::new()
        .route("/api/world-snapshot", get(routes::get_world_snapshot))
        .route("/api/map", get(routes::get_map))
        .route("/api/npcs-here", get(routes::get_npcs_here))
        .route("/api/theme", get(routes::get_theme))
        .route("/api/ui-config", get(routes::get_ui_config))
        .route("/api/debug-snapshot", get(routes::get_debug_snapshot))
        .route("/api/submit-input", post(routes::submit_input))
        .route("/api/react-to-message", post(routes::react_to_message))
        .route("/api/discover-save-files", get(routes::discover_save_files))
        .route("/api/save-game", post(routes::save_game))
        .route("/api/load-branch", post(routes::load_branch))
        .route("/api/create-branch", post(routes::create_branch))
        .route("/api/new-save-file", post(routes::new_save_file))
        .route("/api/new-game", post(routes::new_game))
        .route("/api/save-state", get(routes::get_save_state))
        .route("/api/ws", get(ws::ws_handler))
        // ── Editor routes (Parish Designer) ─────────────────────────────
        .route(
            "/api/editor-list-mods",
            get(editor_routes::editor_list_mods),
        )
        .route("/api/editor-open-mod", post(editor_routes::editor_open_mod))
        .route(
            "/api/editor-get-snapshot",
            get(editor_routes::editor_get_snapshot),
        )
        .route("/api/editor-validate", get(editor_routes::editor_validate))
        .route(
            "/api/editor-update-npcs",
            post(editor_routes::editor_update_npcs),
        )
        .route(
            "/api/editor-update-locations",
            post(editor_routes::editor_update_locations),
        )
        .route("/api/editor-save", post(editor_routes::editor_save))
        .route("/api/editor-reload", post(editor_routes::editor_reload))
        .route("/api/editor-close", post(editor_routes::editor_close))
        .route(
            "/api/editor-list-saves",
            get(editor_routes::editor_list_saves),
        )
        .route(
            "/api/editor-list-branches",
            post(editor_routes::editor_list_branches),
        )
        .route(
            "/api/editor-list-snapshots",
            post(editor_routes::editor_list_snapshots),
        )
        .route(
            "/api/editor-read-snapshot",
            post(editor_routes::editor_read_snapshot),
        )
        .route("/api/auth/status", get(auth::get_auth_status));

    if oauth_enabled {
        app = app
            .route("/auth/login/google", get(auth::login_google))
            .route("/auth/callback/google", get(auth::callback_google))
            .route("/auth/logout", get(auth::logout));
    }

    let app = app
        .fallback_service(ServeDir::new(&static_dir).append_index_html_on_directories(true))
        .layer(axum_mw::from_fn(cf_access_guard))
        .with_state(Arc::clone(&global))
        .layer(axum_mw::from_fn_with_state(
            Arc::clone(&global),
            middleware::session_middleware,
        ));

    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("Parish web server listening on http://{}", addr);
    tracing::info!("Serving static files from {}", static_dir.display());

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}

/// Reads Google OAuth credentials from environment variables.
fn build_oauth_config() -> Option<OAuthConfig> {
    let client_id = std::env::var("GOOGLE_CLIENT_ID")
        .ok()
        .filter(|s| !s.is_empty())?;
    let client_secret = std::env::var("GOOGLE_CLIENT_SECRET")
        .ok()
        .filter(|s| !s.is_empty())?;
    let base_url =
        std::env::var("PARISH_BASE_URL").unwrap_or_else(|_| "http://localhost:3001".to_string());
    Some(OAuthConfig {
        client_id,
        client_secret,
        base_url,
    })
}
/// Builds the local LLM client and config from environment variables.
fn build_client_and_config() -> (
    Option<parish_core::inference::openai_client::OpenAiClient>,
    GameConfig,
) {
    use parish_core::inference::openai_client::OpenAiClient;

    let provider = std::env::var("PARISH_PROVIDER").unwrap_or_else(|_| "simulator".to_string());
    let model = std::env::var("PARISH_MODEL").unwrap_or_default();
    let base_url = std::env::var("PARISH_BASE_URL").unwrap_or_else(|_| {
        parish_core::config::Provider::from_str_loose(&provider)
            .map(|p| p.default_base_url().to_string())
            .unwrap_or_else(|_| "http://localhost:11434".to_string())
    });
    let api_key = std::env::var("PARISH_API_KEY")
        .ok()
        .filter(|s| !s.is_empty());

    let client = if model.is_empty() && provider != "ollama" {
        None
    } else {
        Some(OpenAiClient::new(&base_url, api_key.as_deref()))
    };

    let model_name = if model.is_empty() {
        "qwen3:14b".to_string()
    } else {
        model
    };

    let config = GameConfig {
        provider_name: provider,
        base_url,
        api_key,
        model_name,
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
        flags: FeatureFlags::default(),
        category_rate_limit: [None, None, None, None],
        // Tile-source fields populated in build_app_state from engine config.
        active_tile_source: String::new(),
        tile_sources: Vec::new(),
        reveal_unexplored_locations: false,
    };

    (client, config)
}

/// Cloud LLM environment configuration loaded from `PARISH_CLOUD_*` vars.
struct CloudEnvConfig {
    provider_name: Option<String>,
    model_name: Option<String>,
    api_key: Option<String>,
    base_url: Option<String>,
}

fn build_cloud_client_from_env() -> CloudEnvConfig {
    let provider = std::env::var("PARISH_CLOUD_PROVIDER")
        .ok()
        .filter(|s| !s.is_empty());
    let base_url = std::env::var("PARISH_CLOUD_BASE_URL").unwrap_or_else(|_| {
        provider
            .as_deref()
            .and_then(|p| parish_core::config::Provider::from_str_loose(p).ok())
            .map(|p| p.default_base_url().to_string())
            .unwrap_or_else(|| "https://openrouter.ai/api".to_string())
    });
    let api_key = std::env::var("PARISH_CLOUD_API_KEY")
        .ok()
        .filter(|s| !s.is_empty());
    let model = std::env::var("PARISH_CLOUD_MODEL")
        .ok()
        .filter(|s| !s.is_empty());

    CloudEnvConfig {
        provider_name: provider.or_else(|| api_key.as_ref().map(|_| "openrouter".to_string())),
        model_name: model,
        api_key,
        base_url: Some(base_url),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_client_and_config_defaults() {
        // In test env, PARISH_PROVIDER is usually not set → defaults to "simulator"
        let (_client, config) = build_client_and_config();
        assert_eq!(config.provider_name, "simulator");
    }

    #[test]
    fn build_oauth_config_missing_returns_none() {
        // Ensure env vars are not set in the test environment.
        // SAFETY: single-threaded test; no other thread reads these vars.
        unsafe {
            std::env::remove_var("GOOGLE_CLIENT_ID");
            std::env::remove_var("GOOGLE_CLIENT_SECRET");
        }
        assert!(build_oauth_config().is_none());
    }
}
