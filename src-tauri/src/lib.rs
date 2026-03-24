//! Parish Tauri backend — app setup, state management, and IPC type definitions.
//!
//! The Rust game engine exposes game state to the Svelte frontend via
//! typed Tauri commands ([`commands`]) and events ([`events`]).

pub mod commands;
pub mod events;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tauri::Emitter;
use tokio::sync::Mutex;

use parish_core::inference::openai_client::OpenAiClient;
use parish_core::inference::{InferenceQueue, spawn_inference_worker};
use parish_core::npc::manager::NpcManager;
use parish_core::world::palette::{RawColor, RawPalette, compute_palette};
use parish_core::world::{LocationId, WorldState};

// ── IPC type definitions ─────────────────────────────────────────────────────

/// A serializable snapshot of the world state sent to the frontend.
#[derive(serde::Serialize, Clone)]
pub struct WorldSnapshot {
    /// Name of the player's current location.
    pub location_name: String,
    /// Short prose description of the current location.
    pub location_description: String,
    /// Human-readable time label (e.g. "Morning", "Dusk").
    pub time_label: String,
    /// Current game hour (0–23).
    pub hour: u8,
    /// Current weather description.
    pub weather: String,
    /// Current season name.
    pub season: String,
    /// Optional festival name if today is a festival day.
    pub festival: Option<String>,
    /// Whether the game clock is currently paused.
    pub paused: bool,
}

/// A location node in the map data.
#[derive(serde::Serialize, Clone)]
pub struct MapLocation {
    /// Location ID as a string.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// WGS-84 latitude (0.0 if not geocoded).
    pub lat: f64,
    /// WGS-84 longitude (0.0 if not geocoded).
    pub lon: f64,
    /// Whether this location is adjacent to (or is) the player's position.
    pub adjacent: bool,
}

/// The full map graph sent to the frontend.
#[derive(serde::Serialize, Clone)]
pub struct MapData {
    /// All locations in the graph.
    pub locations: Vec<MapLocation>,
    /// Edges as (source_id, target_id) string pairs.
    pub edges: Vec<(String, String)>,
    /// The player's current location id.
    pub player_location: String,
}

/// Minimal NPC info for the sidebar.
#[derive(serde::Serialize, Clone)]
pub struct NpcInfo {
    /// NPC's name.
    pub name: String,
    /// NPC's occupation.
    pub occupation: String,
    /// NPC's current mood.
    pub mood: String,
}

/// CSS hex-string theme palette derived from `RawPalette`.
#[derive(serde::Serialize, Clone)]
pub struct ThemePalette {
    /// Main background colour (`"#rrggbb"`).
    pub bg: String,
    /// Foreground (text) colour.
    pub fg: String,
    /// Accent colour for highlights and the status bar.
    pub accent: String,
    /// Slightly offset panel background.
    pub panel_bg: String,
    /// Input field background.
    pub input_bg: String,
    /// Border/separator colour.
    pub border: String,
    /// Muted colour for secondary text.
    pub muted: String,
}

impl From<RawPalette> for ThemePalette {
    fn from(raw: RawPalette) -> Self {
        let hex = |c: RawColor| format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b);
        ThemePalette {
            bg: hex(raw.bg),
            fg: hex(raw.fg),
            accent: hex(raw.accent),
            panel_bg: hex(raw.panel_bg),
            input_bg: hex(raw.input_bg),
            border: hex(raw.border),
            muted: hex(raw.muted),
        }
    }
}

// ── Application state ────────────────────────────────────────────────────────

/// Shared mutable game state managed by Tauri.
///
/// Wrapped in `Arc` so background tasks can hold references without
/// borrowing from `tauri::State<'_>` (which is not `'static`).
pub struct AppState {
    /// The game world (clock, player position, graph, weather).
    pub world: Mutex<WorldState>,
    /// NPC manager (all NPCs, tier assignment, schedule ticking).
    pub npc_manager: Mutex<NpcManager>,
    /// Inference request queue (None until the Tauri runtime is ready).
    pub inference_queue: Mutex<Option<InferenceQueue>>,
    /// Local LLM client (None if no provider is configured).
    pub client: Option<OpenAiClient>,
    /// Cloud LLM client for dialogue (None if not configured).
    pub cloud_client: Option<OpenAiClient>,
    /// Model name for NPC dialogue inference.
    pub model_name: String,
}

// ── Data path resolution ─────────────────────────────────────────────────────

/// Finds the `data/` directory by walking up from the current working directory.
///
/// During `cargo tauri dev` the cwd is `src-tauri/`; in production bundles it
/// may be the app resources directory. We walk up at most 3 levels.
fn find_data_dir() -> PathBuf {
    let mut p = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    for _ in 0..4 {
        if p.join("data/parish.json").exists() {
            return p.join("data");
        }
        match p.parent() {
            Some(parent) => p = parent.to_path_buf(),
            None => break,
        }
    }
    // Fallback — let the load functions fail with a clear error
    PathBuf::from("data")
}

// ── Tauri entry point ─────────────────────────────────────────────────────────

/// Called from `main.rs`. Initialises game state and launches the Tauri app.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    dotenvy::dotenv().ok();

    let data_dir = find_data_dir();

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

    // Initial tier assignment
    npc_manager.assign_tiers(world.player_location, &world.graph);

    // Read provider config from env vars (optional)
    let (client, model_name) = build_client_from_env();
    let cloud_client = build_cloud_client_from_env();

    let state = Arc::new(AppState {
        world: Mutex::new(world),
        npc_manager: Mutex::new(npc_manager),
        inference_queue: Mutex::new(None),
        client: client.clone(),
        cloud_client,
        model_name,
    });

    tauri::Builder::default()
        .manage(Arc::clone(&state))
        .invoke_handler(tauri::generate_handler![
            commands::get_world_snapshot,
            commands::get_map,
            commands::get_npcs_here,
            commands::get_theme,
            commands::submit_input,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();

            // Initialise inference queue now that the tokio runtime is running
            if let Some(ref client) = state.client {
                let (tx, rx) = tokio::sync::mpsc::channel(32);
                let _worker = spawn_inference_worker(client.clone(), rx);
                let queue = InferenceQueue::new(tx);
                let state_ref = Arc::clone(&state);
                tokio::spawn(async move {
                    let mut iq = state_ref.inference_queue.lock().await;
                    *iq = Some(queue);
                });
            }

            // ── Background ticks ─────────────────────────────────────────

            // Idle tick: advance world clock every 20 seconds of real time
            let state_tick = Arc::clone(&state);
            let handle_tick = handle.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(20)).await;
                    {
                        let mut world = state_tick.world.lock().await;
                        if !world.clock.is_paused() {
                            world.clock.advance(20); // advance 20 game minutes
                        }
                        let snapshot = crate::commands::get_world_snapshot_inner(&world);
                        let _ = handle_tick.emit(events::EVENT_WORLD_UPDATE, snapshot);
                    }
                    {
                        let world = state_tick.world.lock().await;
                        let mut npc_mgr = state_tick.npc_manager.lock().await;
                        let events = npc_mgr.tick_schedules(&world.clock, &world.graph);
                        if !events.is_empty() {
                            tracing::debug!("NPC schedule tick: {} events", events.len());
                        }
                    }
                }
            });

            // Theme tick: emit updated palette every 500 ms
            let state_theme = Arc::clone(&state);
            let handle_theme = handle.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    let world = state_theme.world.lock().await;
                    use chrono::Timelike;
                    let now = world.clock.now();
                    let raw = compute_palette(
                        now.hour(),
                        now.minute(),
                        world.clock.season(),
                        world.weather,
                    );
                    let palette = ThemePalette::from(raw);
                    let _ = handle_theme.emit(events::EVENT_THEME_UPDATE, palette);
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Parish application");
}

// ── Client initialisation from env ───────────────────────────────────────────

fn build_client_from_env() -> (Option<OpenAiClient>, String) {
    let provider = std::env::var("PARISH_PROVIDER").unwrap_or_else(|_| "ollama".to_string());
    let model = std::env::var("PARISH_MODEL").unwrap_or_default();
    let base_url = std::env::var("PARISH_BASE_URL").unwrap_or_else(|_| match provider.as_str() {
        "ollama" => "http://localhost:11434".to_string(),
        "lmstudio" => "http://localhost:1234".to_string(),
        "openrouter" => "https://openrouter.ai/api/v1".to_string(),
        _ => "http://localhost:11434".to_string(),
    });
    let api_key = std::env::var("PARISH_API_KEY").ok();

    if model.is_empty() && provider != "ollama" {
        return (None, String::new());
    }

    let client = OpenAiClient::new(&base_url, api_key.as_deref());
    let model_name = if model.is_empty() {
        "qwen3:14b".to_string() // Ollama default
    } else {
        model
    };

    (Some(client), model_name)
}

fn build_cloud_client_from_env() -> Option<OpenAiClient> {
    let base_url = std::env::var("PARISH_CLOUD_BASE_URL")
        .unwrap_or_else(|_| "https://openrouter.ai/api/v1".to_string());
    let api_key = std::env::var("PARISH_CLOUD_API_KEY").ok()?;
    Some(OpenAiClient::new(&base_url, Some(&api_key)))
}
