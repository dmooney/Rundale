//! Parish Tauri backend — app setup, state management, and IPC type definitions.
//!
//! The Rust game engine exposes game state to the Svelte frontend via
//! typed Tauri commands ([`commands`]) and events ([`events`]).

pub mod commands;
pub mod events;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use tauri::Emitter;
use tokio::sync::Mutex;

use parish_core::config::Provider;
use parish_core::debug_snapshot::{DebugEvent, InferenceDebug};
use parish_core::game_mod::PronunciationEntry;
use parish_core::inference::openai_client::OpenAiClient;
use parish_core::inference::{
    InferenceLog, InferenceQueue, new_inference_log, spawn_inference_worker,
};
use parish_core::ipc::ConversationLine;
use parish_core::npc::manager::NpcManager;
use parish_core::npc::reactions::ReactionTemplates;
use parish_core::world::transport::TransportConfig;
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
    /// Current game minute (0–59).
    pub minute: u8,
    /// Current weather description.
    pub weather: String,
    /// Current season name.
    pub season: String,
    /// Optional festival name if today is a festival day.
    pub festival: Option<String>,
    /// Whether the game clock is currently paused.
    pub paused: bool,
    /// Game time as milliseconds since Unix epoch (for client-side interpolation).
    pub game_epoch_ms: f64,
    /// Clock speed multiplier (1 real second = speed_factor game seconds).
    pub speed_factor: f64,
    /// Pronunciation hints for Irish names relevant to the current location.
    pub name_hints: Vec<parish_core::npc::LanguageHint>,
    /// Current day of week (e.g. "Monday", "Saturday").
    pub day_of_week: String,
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
    /// Number of graph hops from the player's current location.
    pub hops: u32,
    /// Whether this location is indoors (for tooltip display).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indoor: Option<bool>,
    /// Estimated walking time from the player's current location, in minutes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub travel_minutes: Option<u16>,
    /// Whether the player has visited this location (false = fog-of-war frontier).
    pub visited: bool,
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
    /// Player's WGS-84 latitude (for centering the minimap).
    pub player_lat: f64,
    /// Player's WGS-84 longitude (for centering the minimap).
    pub player_lon: f64,
    /// Edge traversal counts for footprint rendering: `(src_id, dst_id, count)`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edge_traversals: Vec<(String, String, u32)>,
    /// Human-readable transport mode label (e.g. `"on foot"`).
    pub transport_label: String,
    /// Machine identifier for the active transport mode (e.g. `"walking"`).
    pub transport_id: String,
}

// NpcInfo and ThemePalette are defined in parish-core and re-exported here.
pub use parish_core::ipc::{GameConfig, NpcInfo, ThemePalette};

/// Current save state for display in the StatusBar.
#[derive(serde::Serialize, Clone)]
pub struct SaveState {
    /// Filename of the current save file (e.g. "parish_001.db"), or None.
    pub filename: Option<String>,
    /// Current branch database id, or None.
    pub branch_id: Option<i64>,
    /// Current branch name, or None.
    pub branch_name: Option<String>,
}

/// Maximum number of debug events to retain.
pub const DEBUG_EVENT_CAPACITY: usize = 100;

/// UI configuration sent to the frontend via `get_ui_config`.
///
/// Sourced from the loaded [`GameMod`](parish_core::game_mod::GameMod)'s `ui.toml`
/// or defaults if no mod is loaded.
#[derive(serde::Serialize, Clone)]
pub struct UiConfigSnapshot {
    /// Label for the language-hints sidebar panel.
    pub hints_label: String,
    /// Default accent colour (CSS hex string).
    pub default_accent: String,
    /// Splash text displayed on game start (Zork-style).
    pub splash_text: String,
}

/// Runtime conversation/session state used for continuity and inactivity timers.
pub struct ConversationRuntimeState {
    /// Player location associated with the current transcript.
    pub location: Option<LocationId>,
    /// Recent dialogue at the current location.
    pub transcript: std::collections::VecDeque<ConversationLine>,
    /// Last wall-clock moment when the player submitted input.
    pub last_player_activity: Instant,
    /// Last wall-clock moment when anyone spoke at this location.
    pub last_spoken_at: Instant,
    /// Whether an NPC conversation sequence is currently active.
    pub conversation_in_progress: bool,
}

impl Default for ConversationRuntimeState {
    fn default() -> Self {
        Self::new()
    }
}

impl ConversationRuntimeState {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            location: None,
            transcript: std::collections::VecDeque::with_capacity(16),
            last_player_activity: now,
            last_spoken_at: now,
            conversation_in_progress: false,
        }
    }

    pub fn sync_location(&mut self, location: LocationId) {
        if self.location != Some(location) {
            self.location = Some(location);
            self.transcript.clear();
        }
    }

    pub fn push_line(&mut self, line: ConversationLine) {
        if line.text.trim().is_empty() {
            return;
        }
        if self.transcript.len() >= 12 {
            self.transcript.pop_front();
        }
        self.transcript.push_back(line);
    }
}

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
    pub client: Mutex<Option<OpenAiClient>>,
    /// Cloud LLM client for dialogue (None if not configured).
    pub cloud_client: Mutex<Option<OpenAiClient>>,
    /// Mutable runtime configuration (provider, model, cloud, improv).
    pub config: Mutex<GameConfig>,
    /// Local conversation transcript and inactivity tracking.
    pub conversation: Mutex<ConversationRuntimeState>,
    /// Rolling debug event log for the debug panel.
    pub debug_events: Mutex<std::collections::VecDeque<DebugEvent>>,
    /// Shared inference call log for the debug panel.
    pub inference_log: InferenceLog,
    /// UI configuration from the loaded game mod.
    pub ui_config: UiConfigSnapshot,
    /// Fixed theme palette from the loaded game mod.
    pub theme_palette: ThemePalette,
    /// Name pronunciation entries from the loaded game mod.
    pub pronunciations: Vec<PronunciationEntry>,
    /// NPC arrival reaction templates from the loaded game mod.
    pub reaction_templates: ReactionTemplates,
    /// Path to the currently active save database file (None if unsaved).
    pub save_path: Mutex<Option<PathBuf>>,
    /// Branch id within the current save file.
    pub current_branch_id: Mutex<Option<i64>>,
    /// Name of the current branch.
    pub current_branch_name: Mutex<Option<String>>,
    /// Transport mode configuration from the loaded game mod.
    pub transport: TransportConfig,
}

// ── Data path resolution ─────────────────────────────────────────────────────

/// Finds the `data/` directory by walking up from the current working directory.
///
/// During `cargo tauri dev` the cwd is `src-tauri/`; in production bundles it
/// may be the app resources directory. We walk up at most 3 levels.
pub(crate) fn find_data_dir() -> PathBuf {
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

// ── Screenshot helpers ────────────────────────────────────────────────────────

/// Encodes raw RGBA bytes as a PNG file at `path`.
#[cfg(target_os = "linux")]
fn save_png(path: &std::path::Path, rgba: &[u8], width: u32, height: u32) -> anyhow::Result<()> {
    use std::io::BufWriter;
    let file = std::fs::File::create(path)?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(rgba)?;
    Ok(())
}

/// Captures the entire root X11 window (the Xvfb display) via GDK and saves it
/// as a PNG. This works in headless environments because wry/GTK already have
/// a GDK display connection open.
#[cfg(target_os = "linux")]
fn capture_gdk_screenshot(path: &std::path::Path) -> anyhow::Result<()> {
    use gdk::prelude::*;

    // Flush and synchronize with the X server before capturing.
    // WebKit renders via X11 SHM and copies to the window asynchronously;
    // sync() ensures those XCopyArea operations are complete before GetImage.
    if let Some(display) = gdk::Display::default() {
        display.sync();
    }

    let screen = gdk::Screen::default().ok_or_else(|| anyhow::anyhow!("no GDK default screen"))?;
    let root = screen
        .root_window()
        .ok_or_else(|| anyhow::anyhow!("no root window"))?;
    let width = root.width();
    let height = root.height();

    // WindowExtManual::pixbuf wraps gdk_pixbuf_get_from_window
    let pixbuf = root
        .pixbuf(0, 0, width, height)
        .ok_or_else(|| anyhow::anyhow!("pixbuf_get_from_window returned None"))?;

    // Convert RGB (or RGBA) pixbuf to a flat RGBA byte vec for the PNG encoder
    let has_alpha = pixbuf.has_alpha();
    let channels = pixbuf.n_channels() as usize;
    let rowstride = pixbuf.rowstride() as usize;
    let src = pixbuf.read_pixel_bytes();
    let (w, h) = (width as usize, height as usize);
    let mut rgba: Vec<u8> = Vec::with_capacity(w * h * 4);
    for row in 0..h {
        for col in 0..w {
            let offset = row * rowstride + col * channels;
            rgba.push(src[offset]); // R
            rgba.push(src[offset + 1]); // G
            rgba.push(src[offset + 2]); // B
            rgba.push(if has_alpha { src[offset + 3] } else { 255 }); // A
        }
    }

    save_png(path, &rgba, width as u32, height as u32)
}

#[cfg(not(target_os = "linux"))]
#[allow(dead_code)] // Used only on Linux; this stub exists for cross-compilation.
fn capture_gdk_screenshot(_path: &std::path::Path) -> anyhow::Result<()> {
    anyhow::bail!("screenshot capture is only implemented on Linux")
}

/// Dispatches a screenshot to the GTK main thread (Linux) and waits for completion.
///
/// GDK/GTK APIs must be called from the main thread. We post the capture work
/// via `glib::idle_add_once` and block a spawn_blocking thread on the result.
#[cfg(target_os = "linux")]
async fn dispatch_screenshot(path: std::path::PathBuf) -> anyhow::Result<()> {
    let (tx, rx) = std::sync::mpsc::sync_channel::<anyhow::Result<()>>(1);
    glib::idle_add_once(move || {
        let _ = tx.send(capture_gdk_screenshot(&path));
    });
    tokio::task::spawn_blocking(move || {
        rx.recv()
            .unwrap_or_else(|_| anyhow::bail!("channel closed"))
    })
    .await?
}

#[cfg(not(target_os = "linux"))]
async fn dispatch_screenshot(_path: std::path::PathBuf) -> anyhow::Result<()> {
    anyhow::bail!("screenshot capture is only implemented on Linux")
}

// ── Tauri entry point ─────────────────────────────────────────────────────────

/// Called from `main.rs`. Initialises game state and launches the Tauri app.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    dotenvy::dotenv().ok();

    let data_dir = find_data_dir();

    // Parse optional --screenshot <dir> flag.
    // Relative paths are resolved against the workspace root (parent of data/).
    // Path traversal beyond the workspace root is rejected.
    let screenshot_dir: Option<PathBuf> = {
        let args: Vec<String> = std::env::args().collect();
        let workspace_root = data_dir
            .parent()
            .map(|d| d.to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".."));
        args.iter()
            .position(|a| a == "--screenshot")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| {
                let p = PathBuf::from(s);
                let resolved = if p.is_absolute() {
                    p
                } else {
                    // src-tauri/ is one level below the workspace root
                    workspace_root.join(p)
                };
                // Create the directory so canonicalize can resolve it
                std::fs::create_dir_all(&resolved).ok();
                let canonical = match resolved.canonicalize() {
                    Ok(c) => c,
                    Err(_) => {
                        eprintln!("screenshot: could not resolve path: {}", resolved.display());
                        return None;
                    }
                };
                let ws_canonical = match workspace_root.canonicalize() {
                    Ok(c) => c,
                    Err(_) => return None,
                };
                if canonical.starts_with(&ws_canonical) {
                    Some(canonical)
                } else {
                    eprintln!(
                        "screenshot: path escapes workspace root: {}",
                        resolved.display()
                    );
                    None
                }
            })
    };

    // Try to load game mod (auto-detect from workspace root)
    let game_mod = parish_core::game_mod::find_default_mod().and_then(|dir| {
        match parish_core::game_mod::GameMod::load(&dir) {
            Ok(gm) => {
                tracing::info!(
                    "Loaded game mod: {} ({})",
                    gm.manifest.meta.name,
                    dir.display()
                );
                Some(gm)
            }
            Err(e) => {
                tracing::warn!("Failed to load mod from {}: {}", dir.display(), e);
                None
            }
        }
    });

    // Load world — prefer mod data, fall back to legacy data/ directory
    let world = if let Some(ref gm) = game_mod {
        WorldState::from_mod(gm).unwrap_or_else(|e| {
            tracing::warn!("Failed to load world from mod: {}. Using default.", e);
            WorldState::new()
        })
    } else {
        WorldState::from_parish_file(&data_dir.join("parish.json"), LocationId(15)).unwrap_or_else(
            |e| {
                tracing::warn!("Failed to load parish.json: {}. Using default world.", e);
                WorldState::new()
            },
        )
    };

    // Load NPCs — prefer mod data, fall back to legacy data/ directory
    let npcs_path = if let Some(ref gm) = game_mod {
        gm.npcs_path()
    } else {
        data_dir.join("npcs.json")
    };
    let mut npc_manager = NpcManager::load_from_file(&npcs_path).unwrap_or_else(|e| {
        tracing::warn!("Failed to load npcs.json: {}. No NPCs.", e);
        NpcManager::new()
    });

    // Initial tier assignment
    npc_manager.assign_tiers(&world, &[]);

    // Read provider config from env vars (optional)
    let (client, model_name, provider_name, base_url, api_key) = build_client_from_env();
    let cloud_env = build_cloud_client_from_env();

    // Build splash text from mod title + build info
    let game_title = game_mod
        .as_ref()
        .and_then(|gm| gm.manifest.meta.title.clone())
        .unwrap_or_else(|| "Parish".to_string());
    let splash_text = format!(
        "{}\nCopyright \u{00A9} 2026 David Mooney. All rights reserved.\n{} - {}",
        game_title,
        env!("PARISH_GIT_BRANCH"),
        env!("PARISH_BUILD_TIME"),
    );

    // Build transport config from mod or defaults
    let transport = game_mod
        .as_ref()
        .map(|gm| gm.transport.clone())
        .unwrap_or_default();

    let theme_palette = game_mod
        .as_ref()
        .map(|gm| gm.ui.theme.resolved_palette())
        .unwrap_or_else(parish_core::game_mod::default_theme_palette);

    // Build UI config from mod or defaults
    let ui_config = if let Some(ref gm) = game_mod {
        UiConfigSnapshot {
            hints_label: gm.ui.sidebar.hints_label.clone(),
            default_accent: theme_palette.accent.clone(),
            splash_text: splash_text.clone(),
        }
    } else {
        UiConfigSnapshot {
            hints_label: "Language Hints".to_string(),
            default_accent: theme_palette.accent.clone(),
            splash_text,
        }
    };

    // Extract pronunciation data from the game mod
    let pronunciations = game_mod
        .as_ref()
        .map(|gm| gm.pronunciations.clone())
        .unwrap_or_default();

    // Extract reaction templates from the game mod
    let reaction_templates = game_mod
        .as_ref()
        .map(|gm| gm.reactions.clone())
        .unwrap_or_default();

    let state = Arc::new(AppState {
        world: Mutex::new(world),
        npc_manager: Mutex::new(npc_manager),
        inference_queue: Mutex::new(None),
        client: Mutex::new(client.clone()),
        cloud_client: Mutex::new(cloud_env.client),
        conversation: Mutex::new(ConversationRuntimeState::new()),
        debug_events: Mutex::new(std::collections::VecDeque::with_capacity(
            DEBUG_EVENT_CAPACITY,
        )),
        inference_log: new_inference_log(),
        ui_config,
        theme_palette,
        pronunciations,
        reaction_templates,
        save_path: Mutex::new(None),
        current_branch_id: Mutex::new(None),
        current_branch_name: Mutex::new(None),
        transport,
        config: Mutex::new(GameConfig {
            provider_name,
            base_url,
            api_key,
            model_name,
            cloud_provider_name: cloud_env.provider_name,
            cloud_model_name: cloud_env.model_name,
            cloud_api_key: cloud_env.api_key,
            cloud_base_url: cloud_env.base_url,
            improv_enabled: false,
            max_follow_up_turns: 2,
            idle_banter_after_secs: 25,
            auto_pause_after_secs: 60,
            category_provider: [None, None, None, None],
            category_model: [None, None, None, None],
            category_api_key: [None, None, None, None],
            category_base_url: [None, None, None, None],
        }),
    });

    tauri::Builder::default()
        .manage(Arc::clone(&state))
        .invoke_handler(tauri::generate_handler![
            commands::get_world_snapshot,
            commands::get_map,
            commands::get_npcs_here,
            commands::get_theme,
            commands::get_ui_config,
            commands::get_debug_snapshot,
            commands::submit_input,
            commands::discover_save_files,
            commands::save_game,
            commands::load_branch,
            commands::create_branch,
            commands::new_save_file,
            commands::new_game,
            commands::get_save_state,
            commands::react_to_message,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();

            // ── Screenshot mode ───────────────────────────────────────────────
            // If --screenshot <dir> was passed, capture the UI at 4 times of day
            // and exit. No background ticks are started in this mode.
            if let Some(dir) = screenshot_dir.clone() {
                let state_ss = Arc::clone(&state);
                let handle_ss = handle.clone();
                tauri::async_runtime::spawn(async move {
                    // Give the WebView time to fully load the frontend.
                    // In Xvfb + WebKit2 software rendering the JS bundle takes
                    // ~15–20 s to parse, JIT, and complete the initial IPC round-trip
                    // before onMount data is rendered into the DOM.
                    tokio::time::sleep(Duration::from_secs(20)).await;

                    // Emit the configured theme once so the frontend has a palette
                    // painted before the first capture.
                    {
                        let palette = state_ss.theme_palette.clone();
                        let _ = handle_ss.emit(events::EVENT_THEME_UPDATE, palette);
                    }
                    tokio::time::sleep(Duration::from_secs(3)).await;

                    let times: &[(&str, u32)] =
                        &[("morning", 7), ("midday", 12), ("dusk", 18), ("night", 22)];

                    std::fs::create_dir_all(&dir).ok();

                    for (name, target_hour) in times {
                        // Advance clock to target hour
                        {
                            use chrono::Timelike;
                            let mut world = state_ss.world.lock().await;
                            let current_hour = world.clock.now().hour() as i64;
                            let delta = ((*target_hour as i64) - current_hour).rem_euclid(24) * 60;
                            world.clock.advance(delta);
                        }

                        // Wait for Svelte to re-render and WebKit to commit the frame
                        tokio::time::sleep(Duration::from_secs(5)).await;

                        // GDK must be called from the GTK main thread; dispatch and await.
                        let path = dir.join(format!("gui-{}.png", name));
                        if let Err(e) = dispatch_screenshot(path).await {
                            tracing::error!(name = %name, error = %e, "screenshot capture failed");
                        }
                    }

                    println!("screenshot: all done, exiting");
                    handle_ss.exit(0);
                });

                return Ok(());
            }

            // Spawn all background tasks via Tauri's async runtime.
            // The setup callback is synchronous (runs on the GTK event loop thread)
            // so tokio::spawn cannot be called directly here — we must go through
            // tauri::async_runtime::spawn, which uses the Tauri-managed tokio handle.
            let state_setup = Arc::clone(&state);
            tauri::async_runtime::spawn(async move {
                // Initialise inference queue now that the tokio runtime is running
                {
                    let client_guard = state_setup.client.lock().await;
                    if let Some(ref client) = *client_guard {
                        let (tx, rx) = tokio::sync::mpsc::channel(32);
                        let _worker = spawn_inference_worker(
                            client.clone(),
                            rx,
                            state_setup.inference_log.clone(),
                        );
                        let queue = InferenceQueue::new(tx);
                        let mut iq = state_setup.inference_queue.lock().await;
                        *iq = Some(queue);
                    }
                }

                // ── Persistence: auto-load or create save file ──────────────
                {
                    use parish_core::persistence::Database;
                    use parish_core::persistence::picker::{
                        discover_saves, ensure_saves_dir, new_save_path,
                    };
                    use parish_core::persistence::snapshot::GameSnapshot;

                    let saves_dir = {
                        // Anchor saves dir at the project root (where mods/ lives).
                        let mut p = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                        let mut found = None;
                        for _ in 0..4 {
                            if p.join("mods/rundale/world.json").exists() {
                                let sd = p.join("saves");
                                std::fs::create_dir_all(&sd).ok();
                                found = Some(sd);
                                break;
                            }
                            match p.parent() {
                                Some(parent) => p = parent.to_path_buf(),
                                None => break,
                            }
                        }
                        found.unwrap_or_else(ensure_saves_dir)
                    };

                    let world = state_setup.world.lock().await;
                    let saves = discover_saves(&saves_dir, &world.graph);
                    drop(world);

                    if let Some(save) = saves.last() {
                        // Load the most recent save file
                        match Database::open(&save.path) {
                            Ok(db) => {
                                // Find the "main" branch or first branch
                                let branch = db.find_branch("main").ok().flatten().or_else(|| {
                                    db.list_branches().ok().and_then(|b| b.into_iter().next())
                                });

                                if let Some(branch) = branch {
                                    if let Ok(Some((_snap_id, snapshot))) =
                                        db.load_latest_snapshot(branch.id)
                                    {
                                        let mut world = state_setup.world.lock().await;
                                        let mut npc_mgr = state_setup.npc_manager.lock().await;
                                        snapshot.restore(&mut world, &mut npc_mgr);
                                        npc_mgr.assign_tiers(&world, &[]);
                                        drop(npc_mgr);
                                        drop(world);

                                        *state_setup.save_path.lock().await =
                                            Some(save.path.clone());
                                        *state_setup.current_branch_id.lock().await =
                                            Some(branch.id);
                                        *state_setup.current_branch_name.lock().await =
                                            Some(branch.name.clone());
                                        tracing::info!(
                                            "Restored from {} (branch: {})",
                                            save.filename,
                                            branch.name
                                        );
                                    } else {
                                        // Save file exists but no snapshots — save initial state
                                        let world = state_setup.world.lock().await;
                                        let npc_mgr = state_setup.npc_manager.lock().await;
                                        let snap = GameSnapshot::capture(&world, &npc_mgr);
                                        drop(npc_mgr);
                                        drop(world);
                                        let _ = db.save_snapshot(branch.id, &snap);

                                        *state_setup.save_path.lock().await =
                                            Some(save.path.clone());
                                        *state_setup.current_branch_id.lock().await =
                                            Some(branch.id);
                                        *state_setup.current_branch_name.lock().await =
                                            Some(branch.name);
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Failed to open save file {}: {}", save.filename, e);
                            }
                        }
                    } else {
                        // No saves exist — create a new save file
                        let path = new_save_path(&saves_dir);
                        match Database::open(&path) {
                            Ok(db) => {
                                if let Ok(Some(branch)) = db.find_branch("main") {
                                    let world = state_setup.world.lock().await;
                                    let npc_mgr = state_setup.npc_manager.lock().await;
                                    let snap = GameSnapshot::capture(&world, &npc_mgr);
                                    drop(npc_mgr);
                                    drop(world);
                                    let _ = db.save_snapshot(branch.id, &snap);

                                    *state_setup.save_path.lock().await = Some(path);
                                    *state_setup.current_branch_id.lock().await = Some(branch.id);
                                    *state_setup.current_branch_name.lock().await =
                                        Some("main".to_string());
                                    tracing::info!("Created new save file");
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Failed to create save file: {}", e);
                            }
                        }
                    }
                }

                // ── Background ticks ─────────────────────────────────────────

                // Idle tick: emit world snapshot every 5 seconds.
                // The GameClock already flows via speed_factor — no manual advance needed.
                let state_tick = Arc::clone(&state_setup);
                let handle_tick = handle.clone();
                tokio::spawn(async move {
                    loop {
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        {
                            let world = state_tick.world.lock().await;
                            let transport = state_tick.transport.default_mode();
                            let npc_mgr = state_tick.npc_manager.lock().await;
                            let snapshot = crate::commands::get_world_snapshot_inner(
                                &world,
                                transport,
                                Some(&npc_mgr),
                                &state_tick.pronunciations,
                            );
                            let _ = handle_tick.emit(events::EVENT_WORLD_UPDATE, snapshot);
                        }
                        {
                            let mut world = state_tick.world.lock().await;
                            let mut npc_mgr = state_tick.npc_manager.lock().await;

                            // Tick weather engine
                            let season = world.clock.season();
                            let now = world.clock.now();
                            {
                                let mut rng = rand::thread_rng();
                                if let Some(new_weather) =
                                    world.weather_engine.tick(now, season, &mut rng)
                                {
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
                            }

                            let schedule_events =
                                npc_mgr.tick_schedules(&world.clock, &world.graph, world.weather);
                            let tier_transitions = npc_mgr.assign_tiers(&world, &[]);

                            // Log schedule events and tier transitions to debug panel
                            if !schedule_events.is_empty() || !tier_transitions.is_empty() {
                                let mut debug_events = state_tick.debug_events.lock().await;
                                for evt in &schedule_events {
                                    if debug_events.len() >= crate::DEBUG_EVENT_CAPACITY {
                                        debug_events.pop_front();
                                    }
                                    debug_events.push_back(DebugEvent {
                                        timestamp: String::new(),
                                        category: "schedule".to_string(),
                                        message: evt.debug_string(),
                                    });
                                }
                                for tt in &tier_transitions {
                                    if debug_events.len() >= crate::DEBUG_EVENT_CAPACITY {
                                        debug_events.pop_front();
                                    }
                                    let direction =
                                        if tt.promoted { "promoted" } else { "demoted" };
                                    debug_events.push_back(DebugEvent {
                                        timestamp: String::new(),
                                        category: "tier".to_string(),
                                        message: format!(
                                            "{} {} {:?} → {:?}",
                                            tt.npc_name, direction, tt.old_tier, tt.new_tier,
                                        ),
                                    });
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

                // Inactivity tick: drive idle banter and auto-pause.
                let state_idle = Arc::clone(&state_setup);
                let handle_idle = handle.clone();
                tokio::spawn(async move {
                    loop {
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        crate::commands::tick_inactivity(&state_idle, &handle_idle).await;
                    }
                });
                // Debug tick: emit debug snapshot every 2 seconds
                let state_debug = Arc::clone(&state_setup);
                let handle_debug = handle.clone();
                tokio::spawn(async move {
                    loop {
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        let world = state_debug.world.lock().await;
                        let npc_manager = state_debug.npc_manager.lock().await;
                        let debug_events = state_debug.debug_events.lock().await;
                        let config = state_debug.config.lock().await;

                        let call_log: Vec<parish_core::debug_snapshot::InferenceLogEntry> =
                            state_debug
                                .inference_log
                                .lock()
                                .await
                                .iter()
                                .cloned()
                                .collect();

                        let inference = InferenceDebug {
                            provider_name: config.provider_name.clone(),
                            model_name: config.model_name.clone(),
                            base_url: config.base_url.clone(),
                            cloud_provider: config.cloud_provider_name.clone(),
                            cloud_model: config.cloud_model_name.clone(),
                            has_queue: state_debug.inference_queue.lock().await.is_some(),
                            improv_enabled: config.improv_enabled,
                            call_log,
                        };

                        let snapshot = parish_core::debug_snapshot::build_debug_snapshot(
                            &world,
                            &npc_manager,
                            &debug_events,
                            &inference,
                        );
                        let _ = handle_debug.emit(events::EVENT_DEBUG_UPDATE, snapshot);
                    }
                });

                // Autosave tick: save snapshot every 60 seconds (if a save file is active)
                let state_autosave = Arc::clone(&state_setup);
                tokio::spawn(async move {
                    loop {
                        tokio::time::sleep(Duration::from_secs(60)).await;

                        // Only autosave if a save file and branch are active
                        let save_path = state_autosave.save_path.lock().await.clone();
                        let branch_id = *state_autosave.current_branch_id.lock().await;

                        if let (Some(path), Some(bid)) = (save_path, branch_id) {
                            let world = state_autosave.world.lock().await;
                            let npc_manager = state_autosave.npc_manager.lock().await;
                            let snapshot =
                                parish_core::persistence::snapshot::GameSnapshot::capture(
                                    &world,
                                    &npc_manager,
                                );
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
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Parish application");
}

// ── Client initialisation from env ───────────────────────────────────────────

/// Returns `(client, model_name, provider_name, base_url, api_key)`.
fn build_client_from_env() -> (Option<OpenAiClient>, String, String, String, Option<String>) {
    let provider = std::env::var("PARISH_PROVIDER").unwrap_or_else(|_| "ollama".to_string());
    let model = std::env::var("PARISH_MODEL").unwrap_or_default();
    let base_url = std::env::var("PARISH_BASE_URL").unwrap_or_else(|_| {
        Provider::from_str_loose(&provider)
            .map(|p| p.default_base_url().to_string())
            .unwrap_or_else(|_| "http://localhost:11434".to_string())
    });
    let api_key = std::env::var("PARISH_API_KEY")
        .ok()
        .filter(|s| !s.is_empty());

    if model.is_empty() && provider != "ollama" {
        return (None, String::new(), provider, base_url, api_key);
    }

    let client = OpenAiClient::new(&base_url, api_key.as_deref());
    let model_name = if model.is_empty() {
        "qwen3:14b".to_string() // Ollama default
    } else {
        model
    };

    (
        Some(client),
        model_name,
        provider,
        base_url.clone(),
        api_key,
    )
}

/// Resolved cloud provider configuration from environment variables.
struct CloudEnvConfig {
    /// The constructed OpenAI-compatible client (None if no API key).
    client: Option<OpenAiClient>,
    /// Provider name (e.g. "openrouter").
    provider_name: Option<String>,
    /// Model name for cloud dialogue.
    model_name: Option<String>,
    /// API key.
    api_key: Option<String>,
    /// Base URL for the cloud API.
    base_url: Option<String>,
}

fn build_cloud_client_from_env() -> CloudEnvConfig {
    let provider = std::env::var("PARISH_CLOUD_PROVIDER").ok();
    let base_url = std::env::var("PARISH_CLOUD_BASE_URL").unwrap_or_else(|_| {
        provider
            .as_deref()
            .and_then(|p| Provider::from_str_loose(p).ok())
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
