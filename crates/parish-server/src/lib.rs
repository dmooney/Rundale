//! Parish web server — serves the Svelte UI in a browser via axum.
//!
//! Provides the same game experience as the Tauri desktop app, but over
//! standard HTTP + WebSocket so it can run in any browser. Primarily
//! intended for automated Chrome testing via Playwright.

pub mod routes;
pub mod state;
pub mod ws;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::routing::{get, post};
use tower_http::services::ServeDir;

use parish_core::game_mod::{GameMod, find_default_mod};
use parish_core::inference::openai_client::OpenAiClient;
use parish_core::inference::{InferenceQueue, new_inference_log, spawn_inference_worker};
use parish_core::npc::manager::NpcManager;
use parish_core::world::transport::TransportConfig;
use parish_core::world::{LocationId, WorldState};

use state::{AppState, GameConfig, UiConfigSnapshot, build_app_state};

/// Starts the Parish web server on the given port.
///
/// Loads game data from `data_dir`, serves the Svelte frontend from
/// `static_dir` (typically `ui/dist/`), and exposes REST + WebSocket
/// endpoints for the game.
pub async fn run_server(port: u16, data_dir: PathBuf, static_dir: PathBuf) -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // Load world
    let world = WorldState::from_parish_file(&data_dir.join("parish.json"), LocationId(15))
        .unwrap_or_else(|e| {
            tracing::warn!("Failed to load parish.json: {}. Using default world.", e);
            WorldState::new()
        });

    // Load NPCs
    let mut npc_manager =
        NpcManager::load_from_file(&data_dir.join("npcs.json")).unwrap_or_else(|e| {
            tracing::warn!("Failed to load npcs.json: {}. No NPCs.", e);
            NpcManager::new()
        });
    npc_manager.assign_tiers(&world, &[]);

    // Build client from env
    let (client, config) = build_client_and_config();
    let cloud_client = build_cloud_client();

    let transport = TransportConfig::default();

    // Load game mod (if available) for splash text and reaction templates
    let game_mod = find_default_mod().and_then(|dir| GameMod::load(&dir).ok());
    let game_title = game_mod
        .as_ref()
        .and_then(|gm| gm.manifest.meta.title.clone())
        .unwrap_or_else(|| "Parish".to_string());
    let splash_text = format!(
        "{}\nCopyright \u{00A9} 2026 David Mooney. All rights reserved.\nweb-server - {}",
        game_title,
        chrono::Local::now().format("%Y-%m-%d %H:%M"),
    );
    let ui_config = UiConfigSnapshot {
        hints_label: "Language Hints".to_string(),
        default_accent: "#c4a35a".to_string(),
        splash_text,
    };

    let saves_dir = parish_core::persistence::picker::ensure_saves_dir();
    let state = build_app_state(
        world,
        npc_manager,
        client.clone(),
        config,
        cloud_client,
        transport,
        ui_config,
        saves_dir,
        data_dir.clone(),
        game_mod,
    );

    // Initialize inference queue
    if let Some(ref client) = client {
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        let _worker = spawn_inference_worker(client.clone(), rx, new_inference_log());
        let queue = InferenceQueue::new(tx);
        let mut iq = state.inference_queue.lock().await;
        *iq = Some(queue);
    }

    // Spawn background ticks
    spawn_background_ticks(Arc::clone(&state));

    // Build router
    let app = Router::new()
        .route("/api/world-snapshot", get(routes::get_world_snapshot))
        .route("/api/map", get(routes::get_map))
        .route("/api/npcs-here", get(routes::get_npcs_here))
        .route("/api/theme", get(routes::get_theme))
        .route("/api/ui-config", get(routes::get_ui_config))
        .route("/api/debug-snapshot", get(routes::get_debug_snapshot))
        .route("/api/submit-input", post(routes::submit_input))
        .route("/api/react-to-message", post(routes::react_to_message))
        .route("/api/discover-save-files", get(routes::discover_save_files))
        .route("/api/save-game", get(routes::save_game))
        .route("/api/load-branch", post(routes::load_branch))
        .route("/api/create-branch", post(routes::create_branch))
        .route("/api/new-save-file", get(routes::new_save_file))
        .route("/api/new-game", get(routes::new_game))
        .route("/api/save-state", get(routes::get_save_state))
        .route("/api/ws", get(ws::ws_handler))
        .fallback_service(ServeDir::new(&static_dir).append_index_html_on_directories(true))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("Parish web server listening on http://{}", addr);
    tracing::info!("Serving static files from {}", static_dir.display());

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Spawns the background tick tasks (world update + theme update).
fn spawn_background_ticks(state: Arc<AppState>) {
    // Idle tick: broadcast world snapshot every 5 seconds
    let state_tick = Arc::clone(&state);
    tokio::spawn(async move {
        tracing::debug!("World tick task started");
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            {
                let world = state_tick.world.lock().await;
                let npc_manager = state_tick.npc_manager.lock().await;
                let transport = state_tick.transport.default_mode();
                let mut snapshot = parish_core::ipc::snapshot_from_world(&world, transport);
                snapshot.name_hints = parish_core::ipc::compute_name_hints(
                    &world,
                    &npc_manager,
                    &state_tick.pronunciations,
                );
                state_tick.event_bus.emit("world-update", &snapshot);
            }
            {
                let mut world = state_tick.world.lock().await;
                let mut npc_mgr = state_tick.npc_manager.lock().await;

                // Tick weather engine
                let season = world.clock.season();
                let now = world.clock.now();
                let mut rng = rand::thread_rng();
                if let Some(new_weather) = world.weather_engine.tick(now, season, &mut rng) {
                    let old = world.weather;
                    world.weather = new_weather;
                    world.event_bus.publish(
                        parish_core::world::events::GameEvent::WeatherChanged {
                            new_weather: new_weather.to_string(),
                            timestamp: world.clock.now(),
                        },
                    );
                    tracing::info!(old = %old, new = %new_weather, "Weather changed");
                }

                // Tick NPC schedules and assign tiers
                let schedule_events =
                    npc_mgr.tick_schedules(&world.clock, &world.graph, world.weather);
                let tier_transitions = npc_mgr.assign_tiers(&world, &[]);

                if !schedule_events.is_empty() || !tier_transitions.is_empty() {
                    for evt in &schedule_events {
                        tracing::debug!("NPC schedule: {}", evt.debug_string());
                    }
                    for tt in &tier_transitions {
                        let direction = if tt.promoted { "promoted" } else { "demoted" };
                        tracing::debug!(
                            "NPC tier: {} {} {:?} → {:?}",
                            tt.npc_name,
                            direction,
                            tt.old_tier,
                            tt.new_tier,
                        );
                    }
                }

                // Propagate gossip between co-located Tier 2 NPCs
                if !world.gossip_network.is_empty() {
                    let groups = npc_mgr.tier2_groups();
                    let mut rng = rand::thread_rng();
                    for npc_ids in groups.values() {
                        if npc_ids.len() >= 2 {
                            parish_core::npc::ticks::propagate_gossip_at_location(
                                npc_ids,
                                &mut world.gossip_network,
                                &mut rng,
                            );
                        }
                    }
                }
            }
        }
    });

    // Theme tick: broadcast updated palette every 500 ms
    let state_theme = Arc::clone(&state);
    tokio::spawn(async move {
        tracing::debug!("Theme tick task started");
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let world = state_theme.world.lock().await;
            let palette = parish_core::ipc::build_theme(&world);
            state_theme.event_bus.emit("theme-update", &palette);
        }
    });

    // Autosave tick: save snapshot every 60 seconds (if a save file is active)
    let state_autosave = Arc::clone(&state);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;

            let save_path = state_autosave.save_path.lock().await.clone();
            let branch_id = *state_autosave.current_branch_id.lock().await;

            if let (Some(path), Some(bid)) = (save_path, branch_id) {
                let world = state_autosave.world.lock().await;
                let npc_manager = state_autosave.npc_manager.lock().await;
                let snapshot =
                    parish_core::persistence::snapshot::GameSnapshot::capture(&world, &npc_manager);
                drop(npc_manager);
                drop(world);

                match parish_core::persistence::Database::open(&path) {
                    Ok(db) => match db.save_snapshot(bid, &snapshot) {
                        Ok(_) => tracing::debug!("Autosave complete"),
                        Err(e) => tracing::warn!("Autosave failed: {}", e),
                    },
                    Err(e) => tracing::warn!("Autosave DB open failed: {}", e),
                }
            }
        }
    });
}

/// Builds the local LLM client and config from environment variables.
fn build_client_and_config() -> (Option<OpenAiClient>, GameConfig) {
    let provider = std::env::var("PARISH_PROVIDER").unwrap_or_else(|_| "ollama".to_string());
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
        category_provider: [None, None, None, None],
        category_model: [None, None, None, None],
        category_api_key: [None, None, None, None],
        category_base_url: [None, None, None, None],
    };

    (client, config)
}

/// Builds the cloud LLM client from environment variables.
fn build_cloud_client() -> Option<OpenAiClient> {
    let base_url = std::env::var("PARISH_CLOUD_BASE_URL")
        .unwrap_or_else(|_| "https://openrouter.ai/api".to_string());
    let api_key = std::env::var("PARISH_CLOUD_API_KEY")
        .ok()
        .filter(|s| !s.is_empty());

    api_key
        .as_deref()
        .map(|key| OpenAiClient::new(&base_url, Some(key)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_client_and_config_defaults() {
        // In test env, PARISH_PROVIDER is usually not set → defaults to "ollama"
        let (_client, config) = build_client_and_config();
        assert_eq!(config.provider_name, "ollama");
    }
}
