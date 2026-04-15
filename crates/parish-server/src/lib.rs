//! Parish web server — serves the Svelte UI in a browser via axum.
//!
//! Provides the same game experience as the Tauri desktop app, but over
//! standard HTTP + WebSocket so it can run in any browser. Primarily
//! intended for automated Chrome testing via Playwright.

pub mod routes;
pub mod state;
pub mod ws;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{get, post};
use tower_http::services::ServeDir;

use parish_core::debug_snapshot::DebugEvent;
use parish_core::game_mod::{GameMod, find_default_mod};
use parish_core::inference::openai_client::OpenAiClient;
use parish_core::inference::{AnyClient, InferenceQueue, spawn_inference_worker};
use parish_core::npc::manager::NpcManager;
use parish_core::world::transport::TransportConfig;
use parish_core::world::{LocationId, WorldState};

use parish_core::config::FeatureFlags;
use state::{AppState, DEBUG_EVENT_CAPACITY, GameConfig, UiConfigSnapshot, build_app_state};

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
///
/// Loads game data from `data_dir`, serves the Svelte frontend from
/// `static_dir` (typically `ui/dist/`), and exposes REST + WebSocket
/// endpoints for the game.
pub async fn run_server(port: u16, data_dir: PathBuf, static_dir: PathBuf) -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // Load world — try legacy `parish.json` first, then mod `world.json`.
    let world_path = {
        let parish = data_dir.join("parish.json");
        let world = data_dir.join("world.json");
        if parish.exists() { parish } else { world }
    };
    let world = WorldState::from_parish_file(&world_path, LocationId(15)).unwrap_or_else(|e| {
        tracing::warn!("Failed to load world data: {}. Using default world.", e);
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
    let (client, mut config) = build_client_and_config();
    let cloud_env = build_cloud_client_from_env();
    let cloud_client = cloud_env.client;
    config.cloud_provider_name = cloud_env.provider_name;
    config.cloud_model_name = cloud_env.model_name;
    config.cloud_api_key = cloud_env.api_key;
    config.cloud_base_url = cloud_env.base_url;

    let transport = TransportConfig::default();

    // Load game mod (if available) for splash text and reaction templates
    let game_mod = find_default_mod().and_then(|dir| GameMod::load(&dir).ok());
    let game_title = game_mod
        .as_ref()
        .and_then(|gm| gm.manifest.meta.title.clone())
        .unwrap_or_else(|| "Parish".to_string());
    // Railway injects RAILWAY_GIT_COMMIT_SHA at runtime; also accept a
    // generic PARISH_COMMIT_SHA override. Short-hash for display.
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
    let ui_config = if let Some(ref gm) = game_mod {
        UiConfigSnapshot {
            hints_label: gm.ui.sidebar.hints_label.clone(),
            default_accent: theme_palette.accent.clone(),
            splash_text,
        }
    } else {
        UiConfigSnapshot {
            hints_label: "Language Hints".to_string(),
            default_accent: theme_palette.accent.clone(),
            splash_text,
        }
    };

    // Load feature flags from disk and inject into config
    let flags_path = data_dir.join("parish-flags.json");
    config.flags = FeatureFlags::load_from_file(&flags_path);

    let saves_dir = parish_core::persistence::picker::ensure_saves_dir();
    // Capture provider name before config is moved into build_app_state.
    let provider_name = config.provider_name.clone();
    let state = build_app_state(
        world,
        npc_manager,
        client.clone(),
        config,
        cloud_client,
        transport,
        ui_config,
        theme_palette,
        saves_dir,
        data_dir.clone(),
        game_mod,
        flags_path,
    );

    // Initialize inference queue — use the simulator if configured, else the real client.
    let any_client: Option<AnyClient> = if provider_name == "simulator" {
        Some(AnyClient::simulator())
    } else {
        client.map(AnyClient::open_ai)
    };
    if let Some(ac) = any_client {
        let (interactive_tx, interactive_rx) = tokio::sync::mpsc::channel(16);
        let (background_tx, background_rx) = tokio::sync::mpsc::channel(32);
        let (batch_tx, batch_rx) = tokio::sync::mpsc::channel(64);
        // Store the worker JoinHandle on AppState so a later rebuild_inference
        // call can abort this initial worker — otherwise it leaks for the
        // lifetime of the process (bug #231).
        let worker = spawn_inference_worker(
            ac,
            interactive_rx,
            background_rx,
            batch_rx,
            state.inference_log.clone(),
        );
        let queue = InferenceQueue::new(interactive_tx, background_tx, batch_tx);
        let mut iq = state.inference_queue.lock().await;
        *iq = Some(queue);
        drop(iq);
        let mut wh = state.worker_handle.lock().await;
        *wh = Some(worker);
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
        .route("/api/save-game", post(routes::save_game))
        .route("/api/load-branch", post(routes::load_branch))
        .route("/api/create-branch", post(routes::create_branch))
        .route("/api/new-save-file", post(routes::new_save_file))
        .route("/api/new-game", post(routes::new_game))
        .route("/api/save-state", get(routes::get_save_state))
        .route("/api/ws", get(ws::ws_handler))
        .fallback_service(ServeDir::new(&static_dir).append_index_html_on_directories(true))
        .layer(middleware::from_fn(cf_access_guard))
        .with_state(state);

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

/// Spawns the background tick tasks (world update + theme update).
fn spawn_background_ticks(state: Arc<AppState>) {
    // Event bus fan-in: subscribe to world.event_bus and buffer the last N
    // GameEvents in AppState.game_events for the debug panel.
    {
        let state_events = Arc::clone(&state);
        tokio::spawn(async move {
            let mut rx = {
                let world = state_events.world.lock().await;
                world.event_bus.subscribe()
            };
            loop {
                match rx.recv().await {
                    Ok(evt) => {
                        let mut buf = state_events.game_events.lock().await;
                        if buf.len() >= crate::state::DEBUG_EVENT_CAPACITY {
                            buf.pop_front();
                        }
                        buf.push_back(evt);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    // Idle tick: broadcast world snapshot every 5 seconds
    let state_tick = Arc::clone(&state);
    tokio::spawn(async move {
        tracing::debug!("World tick task started");
        let mut last_palette: Option<parish_core::world::palette::RawPalette> = None;
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
                // Emit current time-of-day palette (weather + season tinted)
                {
                    use chrono::Timelike;
                    use parish_core::ipc::ThemePalette;
                    use parish_core::world::palette::compute_palette;
                    let now = world.clock.now();
                    let raw = compute_palette(
                        now.hour(),
                        now.minute(),
                        world.clock.season(),
                        world.weather,
                    );
                    if last_palette != Some(raw) {
                        state_tick
                            .event_bus
                            .emit("theme-update", &ThemePalette::from(raw));
                        last_palette = Some(raw);
                    }
                }
            }
            {
                let mut world = state_tick.world.lock().await;
                let mut npc_mgr = state_tick.npc_manager.lock().await;

                // Tick weather engine
                let season = world.clock.season();
                let now = world.clock.now();
                // Scope thread_rng tightly so it is dropped before any await.
                let new_weather_opt = {
                    let mut rng = rand::thread_rng();
                    world.weather_engine.tick(now, season, &mut rng)
                };
                if let Some(new_weather) = new_weather_opt {
                    let old = world.weather;
                    world.weather = new_weather;
                    world.event_bus.publish(
                        parish_core::world::events::GameEvent::WeatherChanged {
                            new_weather: new_weather.to_string(),
                            timestamp: world.clock.now(),
                        },
                    );
                    tracing::info!(old = %old, new = %new_weather, "Weather changed");
                    // Emit weather debug event
                    let mut debug_events = state_tick.debug_events.lock().await;
                    if debug_events.len() >= DEBUG_EVENT_CAPACITY {
                        debug_events.pop_front();
                    }
                    debug_events.push_back(DebugEvent {
                        timestamp: String::new(),
                        category: "weather".to_string(),
                        message: format!("Weather: {} → {}", old, new_weather),
                    });
                }

                // Tick NPC schedules and assign tiers
                let schedule_events =
                    npc_mgr.tick_schedules(&world.clock, &world.graph, world.weather);
                let tier_transitions = npc_mgr.assign_tiers(&world, &[]);

                if !schedule_events.is_empty() || !tier_transitions.is_empty() {
                    let mut debug_events = state_tick.debug_events.lock().await;
                    for evt in &schedule_events {
                        tracing::debug!("NPC schedule: {}", evt.debug_string());
                        if debug_events.len() >= DEBUG_EVENT_CAPACITY {
                            debug_events.pop_front();
                        }
                        debug_events.push_back(DebugEvent {
                            timestamp: String::new(),
                            category: "schedule".to_string(),
                            message: evt.debug_string(),
                        });
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
                        if debug_events.len() >= DEBUG_EVENT_CAPACITY {
                            debug_events.pop_front();
                        }
                        debug_events.push_back(DebugEvent {
                            timestamp: String::new(),
                            category: "tier".to_string(),
                            message: format!(
                                "{} {} {:?} → {:?}",
                                tt.npc_name, direction, tt.old_tier, tt.new_tier
                            ),
                        });
                    }
                }

                // Propagate gossip between co-located Tier 2 NPCs.
                // Scope thread_rng tightly so it is dropped before any await.
                let total_gossip = if !world.gossip_network.is_empty() {
                    let groups = npc_mgr.tier2_groups();
                    let mut rng = rand::thread_rng();
                    let mut total = 0usize;
                    for npc_ids in groups.values() {
                        if npc_ids.len() >= 2 {
                            total += parish_core::npc::ticks::propagate_gossip_at_location(
                                npc_ids,
                                &mut world.gossip_network,
                                &mut rng,
                            );
                        }
                    }
                    total
                } else {
                    0
                };
                if total_gossip > 0 {
                    let mut debug_events = state_tick.debug_events.lock().await;
                    if debug_events.len() >= DEBUG_EVENT_CAPACITY {
                        debug_events.pop_front();
                    }
                    debug_events.push_back(DebugEvent {
                        timestamp: String::new(),
                        category: "gossip".to_string(),
                        message: format!("{} rumor(s) spread among co-located NPCs", total_gossip),
                    });
                }

                // Dispatch Tier 4 rules engine if enough game time has elapsed.
                // tick_tier4 is sub-ms CPU work; runs inline inside the lock scope.
                if npc_mgr.needs_tier4_tick(now) {
                    let tier4_ids: std::collections::HashSet<parish_core::npc::NpcId> =
                        npc_mgr.tier4_npcs().into_iter().collect();
                    let events = {
                        let mut tier4_refs: Vec<&mut parish_core::npc::Npc> = npc_mgr
                            .npcs_mut()
                            .values_mut()
                            .filter(|n| tier4_ids.contains(&n.id))
                            .collect();
                        let game_date = now.date_naive();
                        let mut rng = rand::thread_rng();
                        parish_core::npc::tier4::tick_tier4(
                            &mut tier4_refs,
                            season,
                            game_date,
                            &mut rng,
                        )
                    };
                    let game_events = npc_mgr.apply_tier4_events(&events, now);
                    // Collect life event descriptions before publishing
                    let life_descriptions: Vec<String> = game_events
                        .iter()
                        .filter_map(|ge| {
                            if let parish_core::world::events::GameEvent::LifeEvent {
                                description,
                                ..
                            } = ge
                            {
                                Some(description.clone())
                            } else {
                                None
                            }
                        })
                        .collect();
                    for evt in game_events {
                        world.event_bus.publish(evt);
                    }
                    npc_mgr.record_tier4_tick(now);
                    tracing::debug!("Tier 4 tick: {} events", events.len());
                    // Emit per-event life_event debug entries + aggregate tier4 entry
                    let mut debug_events = state_tick.debug_events.lock().await;
                    for desc in &life_descriptions {
                        if debug_events.len() >= DEBUG_EVENT_CAPACITY {
                            debug_events.pop_front();
                        }
                        debug_events.push_back(DebugEvent {
                            timestamp: String::new(),
                            category: "life_event".to_string(),
                            message: desc.clone(),
                        });
                    }
                    if debug_events.len() >= DEBUG_EVENT_CAPACITY {
                        debug_events.pop_front();
                    }
                    debug_events.push_back(DebugEvent {
                        timestamp: String::new(),
                        category: "tier4".to_string(),
                        message: format!("Tier 4 tick: {} events", events.len()),
                    });
                }

                // Dispatch Tier 3 batch LLM simulation for distant NPCs.
                // The LLM call can take 10-30 s, so we spawn a detached task
                // and release the world/npc_mgr locks before awaiting.
                if npc_mgr.needs_tier3_tick(now) && !npc_mgr.tier3_in_flight() {
                    use parish_core::npc::ticks::Tier3Snapshot;
                    use parish_core::npc::ticks::tier3_snapshot_from_npc;

                    let tier3_ids = npc_mgr.tier3_npcs();
                    let snapshots: Vec<Tier3Snapshot> = tier3_ids
                        .iter()
                        .filter_map(|id| npc_mgr.get(*id))
                        .map(|npc| tier3_snapshot_from_npc(npc, &world.graph))
                        .collect();

                    if !snapshots.is_empty() {
                        let time_desc = world.clock.time_of_day().to_string();
                        let weather_str = world.weather.to_string();
                        let season_str = format!("{:?}", world.clock.season());
                        let hours = 24u32;

                        npc_mgr.set_tier3_in_flight(true);

                        let state_t3 = Arc::clone(&state_tick);
                        tokio::spawn(async move {
                            // Briefly lock to clone the queue + resolve the model.
                            // NOTE: queue submissions go through the base worker client;
                            // per-category Simulation overrides are not honored for batch
                            // inference. TODO: per-category routing through the queue worker.
                            let (queue_opt, model) = {
                                let cfg = state_t3.config.lock().await;
                                let queue_guard = state_t3.inference_queue.lock().await;
                                let queue = queue_guard.clone();
                                let idx = parish_core::ipc::GameConfig::cat_idx(
                                    parish_core::config::InferenceCategory::Simulation,
                                );
                                let model = cfg.category_model[idx]
                                    .clone()
                                    .unwrap_or_else(|| cfg.model_name.clone());
                                (queue, model)
                            };

                            let Some(queue) = queue_opt else {
                                state_t3.npc_manager.lock().await.set_tier3_in_flight(false);
                                return;
                            };

                            let ctx = parish_core::npc::ticks::Tier3Context {
                                snapshots: &snapshots,
                                queue: &queue,
                                model: &model,
                                time_desc: &time_desc,
                                weather: &weather_str,
                                season: &season_str,
                                hours,
                                batch_size: 0,
                            };

                            let result = parish_core::npc::ticks::tick_tier3(&ctx).await;

                            // Re-acquire locks to apply updates.
                            let mut npc_mgr = state_t3.npc_manager.lock().await;
                            let world = state_t3.world.lock().await;
                            let game_time = world.clock.now();

                            match result {
                                Ok(updates) => {
                                    let _events = parish_core::npc::ticks::apply_tier3_updates(
                                        &updates,
                                        npc_mgr.npcs_mut(),
                                        &world.graph,
                                        game_time,
                                    );
                                    npc_mgr.record_tier3_tick(game_time);
                                    tracing::debug!(
                                        "Tier 3 tick: {} updates applied",
                                        updates.len()
                                    );
                                    let mut debug_events = state_t3.debug_events.lock().await;
                                    if debug_events.len() >= DEBUG_EVENT_CAPACITY {
                                        debug_events.pop_front();
                                    }
                                    debug_events.push_back(DebugEvent {
                                        timestamp: String::new(),
                                        category: "tier3".to_string(),
                                        message: format!("Tier 3 tick: {} updates", updates.len()),
                                    });
                                }
                                Err(e) => {
                                    tracing::warn!("Tier 3 tick failed: {}", e);
                                }
                            }

                            npc_mgr.set_tier3_in_flight(false);
                        });
                    }
                }

                // Dispatch Tier 2 background simulation for nearby NPCs.
                // Submits one LLM call per location group via the priority queue
                // (Background lane, yields to Tier 1 dialogue).
                if npc_mgr.needs_tier2_tick(now) && !npc_mgr.tier2_in_flight() {
                    use parish_core::npc::ticks::{Tier2Group, npc_snapshot_from_npc};

                    let groups_map = npc_mgr.tier2_groups();
                    if !groups_map.is_empty() {
                        // Build owned snapshots inside the lock scope.
                        let groups: Vec<Tier2Group> = groups_map
                            .into_iter()
                            .filter_map(|(loc, npc_ids)| {
                                let location_name = world
                                    .graph
                                    .get(loc)
                                    .map(|d| d.name.clone())
                                    .unwrap_or_else(|| format!("Location {}", loc.0));
                                let npcs: Vec<_> = npc_ids
                                    .iter()
                                    .filter_map(|id| npc_mgr.get(*id))
                                    .map(npc_snapshot_from_npc)
                                    .collect();
                                if npcs.is_empty() {
                                    return None;
                                }
                                Some(Tier2Group {
                                    location: loc,
                                    location_name,
                                    npcs,
                                })
                            })
                            .collect();

                        if !groups.is_empty() {
                            let time_desc = world.clock.time_of_day().to_string();
                            let weather_str = world.weather.to_string();

                            npc_mgr.set_tier2_in_flight(true);

                            let state_t2 = Arc::clone(&state_tick);
                            tokio::spawn(async move {
                                // Briefly lock to clone the queue + resolve model.
                                // NOTE: queue submissions go through the base worker client;
                                // per-category Simulation overrides are not honored for batch
                                // inference. TODO: per-category routing through the queue worker.
                                let (queue_opt, model) = {
                                    let cfg = state_t2.config.lock().await;
                                    let queue_guard = state_t2.inference_queue.lock().await;
                                    let queue = queue_guard.clone();
                                    let idx = parish_core::ipc::GameConfig::cat_idx(
                                        parish_core::config::InferenceCategory::Simulation,
                                    );
                                    let model = cfg.category_model[idx]
                                        .clone()
                                        .unwrap_or_else(|| cfg.model_name.clone());
                                    (queue, model)
                                };

                                let Some(queue) = queue_opt else {
                                    state_t2.npc_manager.lock().await.set_tier2_in_flight(false);
                                    return;
                                };

                                // Submit each group sequentially (one LLM call per group).
                                let mut events = Vec::new();
                                for group in &groups {
                                    if let Some(evt) = parish_core::npc::ticks::run_tier2_for_group(
                                        &queue,
                                        &model,
                                        group,
                                        &time_desc,
                                        &weather_str,
                                    )
                                    .await
                                    {
                                        events.push(evt);
                                    }
                                }

                                // Re-acquire locks to apply events.
                                let mut npc_mgr = state_t2.npc_manager.lock().await;
                                let mut world = state_t2.world.lock().await;
                                let game_time = world.clock.now();

                                for event in &events {
                                    let _dbg = parish_core::npc::ticks::apply_tier2_event(
                                        event,
                                        npc_mgr.npcs_mut(),
                                        game_time,
                                    );
                                    // Push gossip so it can propagate to other NPCs.
                                    parish_core::npc::ticks::create_gossip_from_tier2_event(
                                        event,
                                        &mut world.gossip_network,
                                        game_time,
                                    );
                                }
                                npc_mgr.record_tier2_tick(game_time);
                                npc_mgr.set_tier2_in_flight(false);

                                tracing::debug!(
                                    "Tier 2 tick: {} events from {} groups",
                                    events.len(),
                                    groups.len()
                                );
                                let mut debug_events = state_t2.debug_events.lock().await;
                                if debug_events.len() >= DEBUG_EVENT_CAPACITY {
                                    debug_events.pop_front();
                                }
                                debug_events.push_back(DebugEvent {
                                    timestamp: String::new(),
                                    category: "tier2".to_string(),
                                    message: format!(
                                        "Tier 2 tick: {} events from {} groups",
                                        events.len(),
                                        groups.len()
                                    ),
                                });
                            });
                        }
                    }
                }
            }
        }
    });

    // Inactivity tick: drive idle banter and auto-pause.
    let state_idle = Arc::clone(&state);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            routes::tick_inactivity(&state_idle).await;
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
    };

    (client, config)
}

/// Cloud LLM environment configuration loaded from `PARISH_CLOUD_*` vars.
struct CloudEnvConfig {
    client: Option<OpenAiClient>,
    provider_name: Option<String>,
    model_name: Option<String>,
    api_key: Option<String>,
    base_url: Option<String>,
}

/// Builds the cloud LLM client and associated config from environment variables.
///
/// Without populating `cloud_provider_name`/`cloud_model_name` on the
/// `GameConfig`, `resolve_category_client` would never route Dialogue
/// inference to the cloud client — so even with `PARISH_CLOUD_API_KEY` set,
/// no requests would actually be sent (e.g. on Railway with Groq configured).
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

    let client = api_key
        .as_deref()
        .map(|key| OpenAiClient::new(&base_url, Some(key)));

    CloudEnvConfig {
        client,
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
}
