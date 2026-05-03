//! Parish Tauri backend — app setup, state management, and IPC type definitions.
//!
//! The Rust game engine exposes game state to the Svelte frontend via
//! typed Tauri commands ([`commands`]) and events ([`events`]).

pub mod command_registry;
pub mod commands;
pub mod editor_commands;
pub mod events;

use parish_core::AUTOSAVE_INTERVAL_SECS;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use tauri::Emitter;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use parish_core::config::{FeatureFlags, Provider, ProviderConfig};
use parish_core::debug_snapshot::{DebugEvent, InferenceDebug};
use parish_core::game_mod::PronunciationEntry;
use parish_core::inference::{
    AnyClient, InferenceLog, InferenceQueue, new_inference_log, spawn_inference_worker,
};
use parish_core::ipc::ConversationLine;
use parish_core::npc::manager::NpcManager;
use parish_core::npc::reactions::ReactionTemplates;
use parish_core::world::transport::TransportConfig;
use parish_core::world::{DEFAULT_START_LOCATION, LocationId, WorldState};

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
    /// Whether the game clock is currently player-paused.
    pub paused: bool,
    /// Whether the game clock is frozen while waiting on inference.
    pub inference_paused: bool,
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

/// Configuration for the LLM demo / auto-player mode.
///
/// Read-only after startup; set via `--demo` CLI flags.
pub struct DemoConfig {
    /// Whether to start the demo loop automatically on launch.
    pub auto_start: bool,
    /// Extra prompt instructions loaded from `--demo-prompt <file>`.
    pub extra_prompt: Option<String>,
    /// Seconds to pause between demo turns (default 2.0).
    pub turn_pause_secs: f32,
    /// Maximum number of turns before stopping (None = unlimited).
    pub max_turns: Option<u32>,
}

impl Default for DemoConfig {
    fn default() -> Self {
        Self {
            auto_start: false,
            extra_prompt: None,
            turn_pause_secs: 2.0,
            max_turns: None,
        }
    }
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
    /// Id of the currently-active tile source (matches a `tile_sources` key).
    pub active_tile_source: String,
    /// Registry of available map tile sources, alphabetical by id.
    pub tile_sources: Vec<parish_core::ipc::TileSourceSnapshot>,
    /// How many seconds of inactivity before auto-pausing the game.
    pub auto_pause_timeout_seconds: u64,
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
///
/// # Lock ordering contract
///
/// Several fields are wrapped in [`tokio::sync::Mutex`]. To avoid
/// deadlocks, any code path that acquires more than one of them **must**
/// do so in the following canonical order, from outermost to innermost:
///
/// 1. [`AppState::world`]
/// 2. [`AppState::npc_manager`]
/// 3. [`AppState::conversation`]
/// 4. [`AppState::debug_events`] / [`AppState::game_events`]
/// 5. [`AppState::config`]
/// 6. [`AppState::save_path`] / [`AppState::current_branch_id`] /
///    [`AppState::current_branch_name`]
/// 7. [`AppState::client`] / [`AppState::cloud_client`]
/// 8. [`AppState::inference_log`]
/// 9. [`AppState::inference_queue`]
///
/// Never drop a lock only to re-acquire it in the same critical section —
/// if two locks are needed, hold them both for the duration of the work.
/// See `background tick` in [`run`] for the canonical example of holding
/// `world` and `npc_manager` together through a full tick iteration.
pub struct AppState {
    /// The game world (clock, player position, graph, weather).
    pub world: Mutex<WorldState>,
    /// NPC manager (all NPCs, tier assignment, schedule ticking).
    pub npc_manager: Mutex<NpcManager>,
    /// Inference request queue (None until the Tauri runtime is ready).
    pub inference_queue: Mutex<Option<InferenceQueue>>,
    /// Local LLM client (None if no provider is configured).
    pub client: Mutex<Option<AnyClient>>,
    /// Cloud LLM client for dialogue (None if not configured).
    pub cloud_client: Mutex<Option<AnyClient>>,
    /// Mutable runtime configuration (provider, model, cloud, improv).
    pub config: Mutex<GameConfig>,
    /// Local conversation transcript and inactivity tracking.
    pub conversation: Mutex<ConversationRuntimeState>,
    /// Rolling debug event log for the debug panel.
    pub debug_events: Mutex<std::collections::VecDeque<DebugEvent>>,
    /// Rolling `GameEvent` ring buffer captured from the world event bus.
    /// Populated by a background task that subscribes to `world.event_bus`.
    pub game_events: Mutex<std::collections::VecDeque<parish_core::world::events::GameEvent>>,
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
    /// Data directory used to derive the feature-flags persistence path.
    pub data_dir: PathBuf,
    /// Saves directory resolved once at startup (#771).
    /// Every save/load command reads this rather than re-probing the cwd.
    pub saves_dir: PathBuf,
    /// Handle for the active inference worker task; used to abort it on rebuild.
    pub worker_handle: Mutex<Option<JoinHandle<()>>>,
    /// Editor session — separate from gameplay state, may be empty.
    pub editor: std::sync::Mutex<parish_core::ipc::editor::EditorSession>,
    /// Advisory file lock for the currently active save file.
    pub save_lock: Mutex<Option<parish_core::persistence::SaveFileLock>>,
    /// Child `ollama serve` process handle (no-op for non-Ollama providers).
    /// Stored here so it lives for the app's lifetime — dropping it kills the
    /// server. See [`parish_core::inference::client::OllamaProcess`].
    pub ollama_process: Mutex<parish_core::inference::client::OllamaProcess>,
    /// TOML-configured inference timeouts loaded from `parish.toml` at boot.
    /// Used by rebuild paths so `/provider` switches honour the configured
    /// values instead of falling back to compiled-in defaults. (#417)
    pub inference_config: parish_core::config::InferenceConfig,
    /// Demo / auto-player configuration. Read-only after startup.
    pub demo_config: DemoConfig,
}

// ── Data path resolution ─────────────────────────────────────────────────────

/// Resolves the `data/` directory once at app startup.
///
/// Resolution order:
/// 1. `PARISH_DATA_DIR` environment variable — explicit operator override.
/// 2. Walks up to 4 ancestors of the cwd looking for `data/parish.json`.
/// 3. Falls back to `./data` and lets the load functions fail with a clear error.
///
/// MUST only be called at startup; the result is stored on
/// [`AppState::data_dir`]. Per-handler callers must read from state instead
/// of re-probing the cwd (#771).
pub(crate) fn find_data_dir() -> PathBuf {
    if let Some(explicit) = std::env::var_os("PARISH_DATA_DIR") {
        return PathBuf::from(explicit);
    }
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

/// Maximum time to wait for a screenshot capture to complete before bailing.
///
/// If the GTK main thread is busy or the capture never completes, we bail
/// instead of blocking the task indefinitely.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))] // Only called from dispatch_screenshot (Linux); tests exercise this cross-platform.
const SCREENSHOT_TIMEOUT: Duration = Duration::from_secs(30);

/// Awaits a screenshot result on `rx`, bounded by `timeout`.
///
/// Returns the captured result, or an error if the channel closes or the
/// timeout expires. Extracted so the timeout/close behavior can be unit-tested
/// without GTK.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))] // Only called from dispatch_screenshot (Linux); tests exercise this cross-platform.
async fn await_screenshot_result(
    rx: std::sync::mpsc::Receiver<anyhow::Result<()>>,
    timeout: Duration,
) -> anyhow::Result<()> {
    let blocking = tokio::task::spawn_blocking(move || {
        rx.recv()
            .unwrap_or_else(|_| anyhow::bail!("channel closed"))
    });
    match tokio::time::timeout(timeout, blocking).await {
        Ok(join_result) => join_result?,
        Err(_) => anyhow::bail!("screenshot capture timed out after {}s", timeout.as_secs()),
    }
}

/// Dispatches a screenshot to the GTK main thread (Linux) and waits for completion.
///
/// GDK/GTK APIs must be called from the main thread. We post the capture work
/// via `glib::idle_add_once` and block a spawn_blocking thread on the result.
/// The whole dispatch is bounded by [`SCREENSHOT_TIMEOUT`] so a wedged GTK main
/// thread cannot hang the caller forever.
#[cfg(target_os = "linux")]
async fn dispatch_screenshot(path: std::path::PathBuf) -> anyhow::Result<()> {
    let (tx, rx) = std::sync::mpsc::sync_channel::<anyhow::Result<()>>(1);
    glib::idle_add_once(move || {
        let _ = tx.send(capture_gdk_screenshot(&path));
    });
    await_screenshot_result(rx, SCREENSHOT_TIMEOUT).await
}

#[cfg(not(target_os = "linux"))]
async fn dispatch_screenshot(_path: std::path::PathBuf) -> anyhow::Result<()> {
    anyhow::bail!("screenshot capture is only implemented on Linux")
}

// ── Tauri entry point ─────────────────────────────────────────────────────────

/// Parses `--demo*` CLI flags from an argument list into a [`DemoConfig`].
///
/// Extracted for unit-testability: does not read env vars or touch the filesystem
/// (except via `std::fs::read_to_string` for `--demo-prompt`, which silently
/// returns `None` on error).
pub(crate) fn parse_demo_args(args: &[String]) -> DemoConfig {
    let auto_start = args.iter().any(|a| a == "--demo");
    let extra_prompt = args
        .iter()
        .position(|a| a == "--demo-prompt")
        .and_then(|i| args.get(i + 1))
        .and_then(|p| std::fs::read_to_string(p).ok());
    let turn_pause_secs = args
        .iter()
        .position(|a| a == "--demo-pause")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(2.0);
    let max_turns = args
        .iter()
        .position(|a| a == "--demo-max-turns")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse::<u32>().ok());
    DemoConfig {
        auto_start,
        extra_prompt,
        turn_pause_secs,
        max_turns,
    }
}

/// Called from `main.rs`. Initialises game state and launches the Tauri app.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialise tracing so RUST_LOG is respected (e.g. RUST_LOG=info,parish_tauri_lib=debug).
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    dotenvy::dotenv().ok();

    let data_dir = find_data_dir();

    // Parse optional --demo flags.
    let demo_config: DemoConfig = {
        let args: Vec<String> = std::env::args().collect();
        parse_demo_args(&args)
    };

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
                if let Err(e) = std::fs::create_dir_all(&resolved) {
                    tracing::warn!(path = %resolved.display(), error = %e, "failed to create mod dir");
                }
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
        parish_core::game_mod::world_state_from_mod(gm).unwrap_or_else(|e| {
            tracing::warn!("Failed to load world from mod: {}. Using default.", e);
            WorldState::new()
        })
    } else {
        WorldState::from_parish_file(&data_dir.join("parish.json"), DEFAULT_START_LOCATION)
            .unwrap_or_else(|e| {
                tracing::warn!("Failed to load parish.json: {}. Using default world.", e);
                WorldState::new()
            })
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

    // Load engine config (parish.toml) early so TOML-configured timeouts are
    // available for provider bootstrap and cloud-client construction. (#417)
    let engine_config = parish_core::config::load_engine_config(None);

    // Read provider config from env vars (optional).
    // On Ollama this runs the full install / auto-start / GPU-detect / pull
    // / warmup sequence — so the desktop app matches the CLI on first launch.
    let (provider_config, provider_name, base_url, api_key) = provider_config_from_env();
    let setup_runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime for provider bootstrap");
    let (client, model_name, ollama_process) = setup_runtime
        .block_on(bootstrap_provider(
            &provider_config,
            &engine_config.inference,
        ))
        .unwrap_or_else(|e| {
            tracing::error!("Failed to initialise inference provider: {}", e);
            eprintln!("[Parish] Failed to initialise inference provider: {}", e);
            std::process::exit(1);
        });
    let cloud_env = build_cloud_client_from_env(&engine_config.inference);

    // Build splash text from mod title + build info
    let game_title = game_mod
        .as_ref()
        .and_then(|gm| gm.manifest.meta.title.clone())
        .unwrap_or_else(|| "Parish".to_string());
    let splash_text = format!(
        "{}\nCopyright \u{00A9} 2026 David Mooney. Licensed under GPL-3.0 \u{2014} see LICENSE.\n{} - {}",
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

    // engine_config already loaded above (before provider bootstrap) and
    // includes both map tile-source registry and inference timeouts. (#417)
    let tile_sources_snapshot =
        parish_core::ipc::TileSourceSnapshot::list_from_map_config(&engine_config.map);
    let active_tile_source = engine_config.map.default_tile_source.clone();

    // Build UI config from mod or defaults
    let ui_config = if let Some(ref gm) = game_mod {
        UiConfigSnapshot {
            hints_label: gm.ui.sidebar.hints_label.clone(),
            default_accent: theme_palette.accent.clone(),
            splash_text: splash_text.clone(),
            active_tile_source: active_tile_source.clone(),
            tile_sources: tile_sources_snapshot.clone(),
            auto_pause_timeout_seconds: engine_config.session.auto_pause_after_secs,
        }
    } else {
        UiConfigSnapshot {
            hints_label: "Language Hints".to_string(),
            default_accent: theme_palette.accent.clone(),
            splash_text,
            active_tile_source: active_tile_source.clone(),
            tile_sources: tile_sources_snapshot,
            auto_pause_timeout_seconds: engine_config.session.auto_pause_after_secs,
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

    // Load feature flags from disk
    let flags = FeatureFlags::load_from_file(&data_dir.join("parish-flags.json"));

    let mut game_config = GameConfig {
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
        idle_banter_after_secs: engine_config.session.idle_banter_after_secs,
        auto_pause_after_secs: engine_config.session.auto_pause_after_secs,
        category_provider: [None, None, None, None],
        category_model: [None, None, None, None],
        category_api_key: [None, None, None, None],
        category_base_url: [None, None, None, None],
        flags,
        category_rate_limit: [None, None, None, None],
        active_tile_source,
        tile_sources: engine_config.map.id_label_pairs(),
        reveal_unexplored_locations: false,
    };
    // Enable demo-mode flag when --demo was passed so the demo commands work.
    if demo_config.auto_start {
        game_config.flags.enable("demo-mode");
    }
    // Fill any unset model fields from the chosen provider's presets so a
    // user who set only `PARISH_PROVIDER=anthropic` (or `--provider`) gets
    // sensible Dialogue/Simulation/Intent/Reaction defaults.
    game_config.fill_missing_models_from_presets();

    // Resolve the saves directory once at startup (#771). Subsequent save/load
    // commands read `state.saves_dir` instead of re-probing the cwd.
    let saves_dir = parish_core::persistence::picker::resolve_project_saves_dir_from_cwd();

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
        game_events: Mutex::new(std::collections::VecDeque::with_capacity(
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
        data_dir: data_dir.clone(),
        saves_dir,
        worker_handle: Mutex::new(None),
        editor: std::sync::Mutex::new(parish_core::ipc::editor::EditorSession::default()),
        save_lock: Mutex::new(None),
        ollama_process: Mutex::new(ollama_process),
        inference_config: engine_config.inference, // (#417) store TOML-configured timeouts
        config: Mutex::new(game_config),
        demo_config,
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
            commands::get_demo_config,
            commands::get_demo_context,
            commands::get_llm_player_action,
            editor_commands::editor_list_mods,
            editor_commands::editor_open_mod,
            editor_commands::editor_get_snapshot,
            editor_commands::editor_validate,
            editor_commands::editor_update_npcs,
            editor_commands::editor_update_locations,
            editor_commands::editor_save,
            editor_commands::editor_reload,
            editor_commands::editor_close,
            editor_commands::editor_list_saves,
            editor_commands::editor_list_branches,
            editor_commands::editor_list_snapshots,
            editor_commands::editor_read_snapshot,
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

                    if let Err(e) = std::fs::create_dir_all(&dir) {
                        tracing::warn!(path = %dir.display(), error = %e, "failed to create screenshot dir");
                    }

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
                    let provider_name = {
                        let config = state_setup.config.lock().await;
                        config.provider_name.clone()
                    };
                    let any_client: Option<AnyClient> = if provider_name == "simulator" {
                        Some(AnyClient::simulator())
                    } else {
                        let client_guard = state_setup.client.lock().await;
                        // `state.client` is already an AnyClient — just clone it.
                        client_guard.as_ref().cloned()
                    };
                    if let Some(ac) = any_client {
                        let (interactive_tx, interactive_rx) =
                            tokio::sync::mpsc::channel(16);
                        let (background_tx, background_rx) =
                            tokio::sync::mpsc::channel(32);
                        let (batch_tx, batch_rx) = tokio::sync::mpsc::channel(64);
                        let worker = spawn_inference_worker(
                            ac,
                            interactive_rx,
                            background_rx,
                            batch_rx,
                            state_setup.inference_log.clone(),
                            state_setup.inference_config.clone(),
                        );
                        let queue =
                            InferenceQueue::new(interactive_tx, background_tx, batch_tx);
                        let mut iq = state_setup.inference_queue.lock().await;
                        *iq = Some(queue);
                        drop(iq);
                        let mut wh = state_setup.worker_handle.lock().await;
                        *wh = Some(worker);
                    }
                }

                // ── Persistence: auto-load or create save file ──────────────
                {
                    use parish_core::persistence::Database;
                    use parish_core::persistence::SaveFileLock;
                    use parish_core::persistence::picker::{discover_saves, new_save_path};
                    use parish_core::persistence::snapshot::GameSnapshot;

                    let saves_dir = state_setup.saves_dir.clone();

                    let world = state_setup.world.lock().await;
                    let saves = discover_saves(&saves_dir, &world.graph);
                    drop(world);

                    // Find the most recent unlocked save (iterate in reverse).
                    let unlocked_save = saves.iter().rev().find(|s| !s.locked);

                    if let Some(save) = unlocked_save {
                        // Acquire the advisory lock before loading.
                        let lock = SaveFileLock::try_acquire(&save.path);
                        if lock.is_some() {
                            *state_setup.save_lock.lock().await = lock;
                        }

                        // Load the most recent unlocked save file
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
                    } else if saves.is_empty() {
                        // No saves exist — create a new save file
                        let path = new_save_path(&saves_dir);
                        let lock = SaveFileLock::try_acquire(&path);
                        if lock.is_some() {
                            *state_setup.save_lock.lock().await = lock;
                        }
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
                    } else {
                        // All saves are locked by other instances.
                        // Show the save picker so the user can choose or create a new ledger.
                        tracing::info!(
                            "All {} save file(s) are locked by other instances — opening save picker",
                            saves.len()
                        );
                        let _ = handle.emit(events::EVENT_SAVE_PICKER, ());
                    }
                }

                // ── Background ticks ─────────────────────────────────────────

                // Event bus fan-in: subscribe to world.event_bus and buffer the
                // last N events in AppState.game_events for the debug panel.
                {
                    let state_events = Arc::clone(&state_setup);
                    let mut rx = {
                        let world = state_events.world.lock().await;
                        world.event_bus.subscribe()
                    };
                    tokio::spawn(async move {
                        loop {
                            match rx.recv().await {
                                Ok(evt) => {
                                    let mut buf = state_events.game_events.lock().await;
                                    if buf.len() >= DEBUG_EVENT_CAPACITY {
                                        buf.pop_front();
                                    }
                                    buf.push_back(evt);
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                    continue;
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                    break;
                                }
                            }
                        }
                    });
                }

                // Idle tick: emit world snapshot and run world/NPC ticks every 5 seconds.
                // The GameClock already flows via speed_factor — no manual advance needed.
                //
                // Lock ordering: `world` → `npc_manager` → `debug_events`. Both
                // `world` and `npc_manager` are acquired once at the top of each
                // iteration and held through the entire body to avoid any window
                // where a command handler could sneak in between them and race
                // the tick (see the AppState lock ordering contract).
                let state_tick = Arc::clone(&state_setup);
                let handle_tick = handle.clone();
                tokio::spawn(async move {
                    let mut last_palette: Option<parish_palette::RawPalette> = None;
                    loop {
                        tokio::time::sleep(Duration::from_secs(5)).await;

                        let mut world = state_tick.world.lock().await;
                        let mut npc_mgr = state_tick.npc_manager.lock().await;

                        // Emit a fresh world snapshot to the frontend.
                        {
                            let transport = state_tick.transport.default_mode();
                            let snapshot = crate::commands::get_world_snapshot_inner(
                                &world,
                                transport,
                                Some(&npc_mgr),
                                &state_tick.pronunciations,
                            );
                            let _ = handle_tick.emit(events::EVENT_WORLD_UPDATE, snapshot);
                            // Emit current time-of-day palette
                            {
                                use chrono::Timelike;
                                use parish_palette::compute_palette;
                                let now = world.clock.now();
                                let raw = compute_palette(now.hour(), now.minute());
                                if last_palette != Some(raw) {
                                    let _ = handle_tick.emit(
                                        events::EVENT_THEME_UPDATE,
                                        ThemePalette::from(raw),
                                    );
                                    last_palette = Some(raw);
                                }
                            }
                        }
                        {
                            // Tick weather engine
                            let season = world.clock.season();
                            let now = world.clock.now();
                            // Scope thread_rng tightly so it is dropped before any await.
                            let new_weather_opt = {
                                let mut rng = rand::rng();
                                world.weather_engine.tick(now, season, &mut rng)
                            };
                            {
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
                                    let mut debug_events =
                                        state_tick.debug_events.lock().await;
                                    if debug_events.len() >= crate::DEBUG_EVENT_CAPACITY {
                                        debug_events.pop_front();
                                    }
                                    debug_events.push_back(DebugEvent {
                                        timestamp: String::new(),
                                        category: "weather".to_string(),
                                        message: format!(
                                            "Weather: {} → {}",
                                            old, new_weather
                                        ),
                                    });
                                }
                            }

                            let schedule_events =
                                npc_mgr.tick_schedules(&world.clock, &world.graph, world.weather);
                            let tier_transitions = npc_mgr.assign_tiers(&world, &[]);

                            // Banshee tick — herald and finalise doomed NPCs.
                            // Default-on; kill-switched by the `banshee` feature flag.
                            let banshee_enabled = {
                                let cfg = state_tick.config.lock().await;
                                !cfg.flags.is_disabled("banshee")
                            };
                            let banshee_report = if banshee_enabled {
                                let world_ref = &mut *world;
                                npc_mgr.tick_banshee(
                                    &world_ref.clock,
                                    &world_ref.graph,
                                    &mut world_ref.text_log,
                                    &world_ref.event_bus,
                                    world_ref.player_location,
                                )
                            } else {
                                parish_core::npc::banshee::BansheeReport::default()
                            };
                            if !banshee_report.is_empty() {
                                let mut debug_events =
                                    state_tick.debug_events.lock().await;
                                if debug_events.len() >= crate::DEBUG_EVENT_CAPACITY {
                                    debug_events.pop_front();
                                }
                                debug_events.push_back(DebugEvent {
                                    timestamp: world.clock.now().format("%H:%M %Y-%m-%d").to_string(),
                                    category: "banshee".to_string(),
                                    message: format!(
                                        "{} wail(s), {} death(s)",
                                        banshee_report.wails.len(),
                                        banshee_report.deaths.len()
                                    ),
                                });
                            }

                            // Log schedule events and tier transitions to debug panel
                            if !schedule_events.is_empty() || !tier_transitions.is_empty() {
                                let ts =
                                    world.clock.now().format("%H:%M %Y-%m-%d").to_string();
                                let mut debug_events = state_tick.debug_events.lock().await;
                                for evt in &schedule_events {
                                    if debug_events.len() >= crate::DEBUG_EVENT_CAPACITY {
                                        debug_events.pop_front();
                                    }
                                    debug_events.push_back(DebugEvent {
                                        timestamp: ts.clone(),
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
                                        timestamp: ts.clone(),
                                        category: "tier".to_string(),
                                        message: format!(
                                            "{} {} {:?} → {:?}",
                                            tt.npc_name, direction, tt.old_tier, tt.new_tier,
                                        ),
                                    });
                                }
                            }

                            // Propagate gossip between co-located Tier 2 NPCs
                            // Scope thread_rng tightly so it is dropped before any await.
                            let total_gossip = if !world.gossip_network.is_empty() {
                                let groups = npc_mgr.tier2_groups();
                                let mut rng = rand::rng();
                                let mut total = 0usize;
                                for npc_ids in groups.values() {
                                    if npc_ids.len() >= 2 {
                                        total +=
                                            parish_core::npc::ticks::propagate_gossip_at_location(
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
                            {
                                if total_gossip > 0 {
                                    let mut debug_events =
                                        state_tick.debug_events.lock().await;
                                    if debug_events.len() >= crate::DEBUG_EVENT_CAPACITY {
                                        debug_events.pop_front();
                                    }
                                    debug_events.push_back(DebugEvent {
                                        timestamp: String::new(),
                                        category: "gossip".to_string(),
                                        message: format!(
                                            "{} rumor(s) spread among co-located NPCs",
                                            total_gossip
                                        ),
                                    });
                                }
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
                                    let mut rng = rand::rng();
                                    parish_core::npc::tier4::tick_tier4(
                                        &mut tier4_refs,
                                        season,
                                        game_date,
                                        &mut rng,
                                    )
                                };
                                let game_events = npc_mgr.apply_tier4_events(&events, now, banshee_enabled);
                                // Collect per-event descriptions before publishing.
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
                                let mut debug_events = state_tick.debug_events.lock().await;
                                // Per-event life_event entries
                                for desc in &life_descriptions {
                                    if debug_events.len() >= crate::DEBUG_EVENT_CAPACITY {
                                        debug_events.pop_front();
                                    }
                                    debug_events.push_back(DebugEvent {
                                        timestamp: String::new(),
                                        category: "life_event".to_string(),
                                        message: desc.clone(),
                                    });
                                }
                                // Aggregate tier4 entry
                                if debug_events.len() >= crate::DEBUG_EVENT_CAPACITY {
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
                            if npc_mgr.needs_tier3_tick(now)
                                && !npc_mgr.tier3_in_flight()
                            {
                                use parish_core::npc::ticks::tier3_snapshot_from_npc;
                                use parish_core::npc::ticks::Tier3Snapshot;

                                let tier3_ids = npc_mgr.tier3_npcs();
                                let snapshots: Vec<Tier3Snapshot> = tier3_ids
                                    .iter()
                                    .filter_map(|id| npc_mgr.get(*id))
                                    .map(|npc| tier3_snapshot_from_npc(npc, &world.graph))
                                    .collect();

                                if !snapshots.is_empty() {
                                    let time_desc =
                                        world.clock.time_of_day().to_string();
                                    let weather_str = world.weather.to_string();
                                    let season_str =
                                        format!("{:?}", world.clock.season());
                                    let hours = 24u32;

                                    npc_mgr.set_tier3_in_flight(true);

                                    let state_t3 = Arc::clone(&state_tick);
                                    tokio::spawn(async move {
                                        // Briefly lock to clone the queue + resolve the model.
                                        // NOTE: queue submissions go through the base worker
                                        // client; per-category Simulation overrides are not
                                        // honored for batch inference. TODO: per-category
                                        // routing through the queue worker.
                                        let (queue_opt, model) = {
                                            let cfg = state_t3.config.lock().await;
                                            let queue_guard =
                                                state_t3.inference_queue.lock().await;
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
                                            state_t3
                                                .npc_manager
                                                .lock()
                                                .await
                                                .set_tier3_in_flight(false);
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

                                        let result =
                                            parish_core::npc::ticks::tick_tier3(&ctx)
                                                .await;

                                        // Re-acquire locks to apply updates.
                                        // Lock ordering: `world` → `npc_manager`
                                        // (matches the documented contract and the
                                        // main tick at lib.rs:955-956).  Acquiring
                                        // npc_manager first while a concurrent main
                                        // tick holds world would deadlock (#337).
                                        let world = state_t3.world.lock().await;
                                        let mut npc_mgr =
                                            state_t3.npc_manager.lock().await;
                                        let game_time = world.clock.now();

                                        match result {
                                            Ok(updates) => {
                                                let _events =
                                                    parish_core::npc::ticks::apply_tier3_updates(
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

                                                let mut debug_events =
                                                    state_t3.debug_events.lock().await;
                                                if debug_events.len()
                                                    >= crate::DEBUG_EVENT_CAPACITY
                                                {
                                                    debug_events.pop_front();
                                                }
                                                debug_events.push_back(DebugEvent {
                                                    timestamp: String::new(),
                                                    category: "tier3".to_string(),
                                                    message: format!(
                                                        "Tier 3 tick: {} updates",
                                                        updates.len()
                                                    ),
                                                });
                                            }
                                            Err(e) => {
                                                tracing::warn!(
                                                    "Tier 3 tick failed: {}",
                                                    e
                                                );
                                            }
                                        }

                                        npc_mgr.set_tier3_in_flight(false);
                                    });
                                }
                            }

                            // Dispatch Tier 2 background simulation for nearby NPCs.
                            // Submits one LLM call per location group via the priority queue
                            // (Background lane, yields to Tier 1 dialogue).
                            if npc_mgr.needs_tier2_tick(now)
                                && !npc_mgr.tier2_in_flight()
                            {
                                use parish_core::npc::ticks::{
                                    Tier2Group, npc_snapshot_from_npc,
                                };

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
                                                .unwrap_or_else(|| {
                                                    format!("Location {}", loc.0)
                                                });
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
                                        let time_desc =
                                            world.clock.time_of_day().to_string();
                                        let weather_str = world.weather.to_string();

                                        npc_mgr.set_tier2_in_flight(true);

                                        let state_t2 = Arc::clone(&state_tick);
                                        tokio::spawn(async move {
                                            // Briefly lock to clone the queue + resolve model.
                                            // NOTE: queue submissions go through the base worker
                                            // client; per-category Simulation overrides are not
                                            // honored for batch inference. TODO: per-category
                                            // routing through the queue worker.
                                            let (queue_opt, model) = {
                                                let cfg = state_t2.config.lock().await;
                                                let queue_guard =
                                                    state_t2.inference_queue.lock().await;
                                                let queue = queue_guard.clone();
                                                let idx =
                                                    parish_core::ipc::GameConfig::cat_idx(
                                                        parish_core::config::InferenceCategory::Simulation,
                                                    );
                                                let model = cfg.category_model[idx]
                                                    .clone()
                                                    .unwrap_or_else(|| {
                                                        cfg.model_name.clone()
                                                    });
                                                (queue, model)
                                            };

                                            let Some(queue) = queue_opt else {
                                                state_t2
                                                    .npc_manager
                                                    .lock()
                                                    .await
                                                    .set_tier2_in_flight(false);
                                                return;
                                            };

                                            // Submit each group sequentially (one LLM call
                                            // per group, single connection).
                                            let mut events = Vec::new();
                                            for group in &groups {
                                                if let Some(evt) =
                                                    parish_core::npc::ticks::run_tier2_for_group(
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
                                            // Lock ordering: `world` → `npc_manager`
                                            // (matches the documented contract and the
                                            // main tick at lib.rs:955-956).  Acquiring
                                            // npc_manager first while a concurrent main
                                            // tick holds world would deadlock (#337).
                                            let mut world = state_t2.world.lock().await;
                                            let mut npc_mgr =
                                                state_t2.npc_manager.lock().await;
                                            let game_time = world.clock.now();

                                            for event in &events {
                                                let _dbg =
                                                    parish_core::npc::ticks::apply_tier2_event(
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

                                            let mut debug_events =
                                                state_t2.debug_events.lock().await;
                                            if debug_events.len()
                                                >= crate::DEBUG_EVENT_CAPACITY
                                            {
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

                        // Advance the generation counter so handle_game_input can
                        // detect TOCTOU races (see issue #283).
                        world.increment_tick_generation();
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
                // Debug tick: emit debug snapshot every 2 seconds.
                //
                // Snapshot each piece of state with a brief, non-overlapping
                // lock window to avoid holding all 5+ locks simultaneously
                // (#105, #282). Lock order: world → npc_manager →
                // inference_queue → config → debug_events → game_events →
                // inference_log (#483).
                let state_debug = Arc::clone(&state_setup);
                let handle_debug = handle.clone();
                tokio::spawn(async move {
                    loop {
                        tokio::time::sleep(Duration::from_secs(2)).await;

                        // 1. Peek inference_queue presence first (#483).
                        let has_inference_queue =
                            state_debug.inference_queue.lock().await.is_some();

                        // 2. Clone config fields — drop the lock immediately.
                        let (
                            provider_name,
                            model_name,
                            base_url,
                            cloud_provider,
                            cloud_model,
                            improv_enabled,
                            categories,
                        ) = {
                            let config = state_debug.config.lock().await;
                            (
                                config.provider_name.clone(),
                                config.model_name.clone(),
                                config.base_url.clone(),
                                config.cloud_provider_name.clone(),
                                config.cloud_model_name.clone(),
                                config.improv_enabled,
                                parish_core::debug_snapshot::build_inference_categories(&config),
                            )
                        };

                        // 3. Clone debug_events ring buffer — drop immediately.
                        let debug_events_snapshot: std::collections::VecDeque<
                            parish_core::debug_snapshot::DebugEvent,
                        > = state_debug
                            .debug_events
                            .lock()
                            .await
                            .iter()
                            .cloned()
                            .collect();

                        // 4. Clone game_events ring buffer — drop immediately.
                        let game_events_snapshot: std::collections::VecDeque<
                            parish_core::world::events::GameEvent,
                        > = state_debug
                            .game_events
                            .lock()
                            .await
                            .iter()
                            .cloned()
                            .collect();

                        // 5. Clone inference log — drop immediately.
                        let call_log: Vec<parish_core::debug_snapshot::InferenceLogEntry> =
                            state_debug
                                .inference_log
                                .lock()
                                .await
                                .iter()
                                .cloned()
                                .collect();

                        // Build InferenceDebug from cloned data (no locks held).
                        let inference = InferenceDebug {
                            provider_name,
                            model_name,
                            base_url,
                            cloud_provider,
                            cloud_model,
                            has_queue: has_inference_queue,
                            reaction_req_id: parish_core::game_session::reaction_req_id_peek(),
                            improv_enabled,
                            call_log,
                            categories,
                            configured_providers: parish_core::debug_snapshot::build_configured_providers(),
                        };

                        // 6. Acquire world and npc_manager (canonical order)
                        // only for the pure-read snapshot build, then release.
                        let world = state_debug.world.lock().await;
                        let npc_manager = state_debug.npc_manager.lock().await;
                        let snapshot = parish_core::debug_snapshot::build_debug_snapshot(
                            &world,
                            &npc_manager,
                            &debug_events_snapshot,
                            &game_events_snapshot,
                            &inference,
                            &parish_core::debug_snapshot::AuthDebug::disabled(),
                        );
                        drop(npc_manager);
                        drop(world);

                        let _ = handle_debug.emit(events::EVENT_DEBUG_UPDATE, snapshot);
                    }
                });

                // Autosave tick: save snapshot every AUTOSAVE_INTERVAL_SECS (if a save file is active)
                let state_autosave = Arc::clone(&state_setup);
                tokio::spawn(async move {
                    loop {
                        tokio::time::sleep(Duration::from_secs(AUTOSAVE_INTERVAL_SECS)).await;

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

/// Reads configuration from `parish.toml` (if present) and `PARISH_*` env vars
/// into a [`ProviderConfig`] plus the display strings that populate [`GameConfig`].
fn provider_config_from_env() -> (ProviderConfig, String, String, Option<String>) {
    let config =
        parish_core::config::resolve_config(None, &Default::default()).unwrap_or_else(|e| {
            tracing::warn!(
                "Failed to resolve configuration: {}; falling back to defaults",
                e
            );
            ProviderConfig {
                provider: parish_core::config::Provider::default(),
                base_url: "http://localhost:11434".to_string(),
                api_key: None,
                model: None,
            }
        });

    let provider_name = config.provider_display();
    let base_url = config.base_url.clone();
    let api_key = config.api_key.clone();

    (config, provider_name, base_url, api_key)
}

/// Runs the full provider bootstrap (Ollama install / auto-start / GPU
/// detect / model pull / warmup when applicable) and returns the ready
/// client, the resolved model tag, and the child-process handle that must
/// live for the app's lifetime.
///
/// Shared plumbing with the CLI and web-server paths via
/// [`parish_core::inference::setup::setup_provider_client`] — CLAUDE.md
/// rule #2 (mode parity).
async fn bootstrap_provider(
    config: &ProviderConfig,
    inference_config: &parish_core::config::InferenceConfig,
) -> anyhow::Result<(
    Option<AnyClient>,
    String,
    parish_core::inference::client::OllamaProcess,
)> {
    // Non-Ollama providers without a model → leave the client unset so the
    // UI can surface a config error instead of failing hard at startup.
    if config.model.is_none() && config.provider != Provider::Ollama {
        return Ok((
            None,
            String::new(),
            parish_core::inference::client::OllamaProcess::none(),
        ));
    }

    let progress = parish_core::inference::setup::StdoutProgress;
    let (client, model, process) = parish_core::inference::setup::setup_provider_client(
        config,
        inference_config, // (#417) use TOML-configured timeouts
        &progress,
    )
    .await?;
    Ok((Some(client), model, process))
}

/// Resolved cloud provider configuration from environment variables.
struct CloudEnvConfig {
    /// The constructed client (None if no API key).
    client: Option<AnyClient>,
    /// Provider name (e.g. "openrouter").
    provider_name: Option<String>,
    /// Model name for cloud dialogue.
    model_name: Option<String>,
    /// API key.
    api_key: Option<String>,
    /// Base URL for the cloud API.
    base_url: Option<String>,
}

fn build_cloud_client_from_env(
    inference_config: &parish_core::config::InferenceConfig,
) -> CloudEnvConfig {
    let provider = std::env::var("PARISH_CLOUD_PROVIDER").ok();
    let base_url = std::env::var("PARISH_CLOUD_BASE_URL").unwrap_or_else(|_| {
        provider
            .as_deref()
            .and_then(|p| Provider::from_str_loose(p).ok())
            .map(|p| p.default_base_url().to_string())
            .unwrap_or_else(|| "https://openrouter.ai/api".to_string())
    });
    let provider_enum = provider
        .as_deref()
        .and_then(|p| Provider::from_str_loose(p).ok())
        .unwrap_or(Provider::OpenRouter);
    let api_key = provider_enum
        .api_key_env_var()
        .and_then(|var| std::env::var(var).ok())
        .filter(|s| !s.is_empty());
    let model = std::env::var("PARISH_CLOUD_MODEL")
        .ok()
        .filter(|s| !s.is_empty());

    let client = api_key.as_deref().map(|key| {
        parish_core::inference::build_client(&provider_enum, &base_url, Some(key), inference_config)
    });

    CloudEnvConfig {
        client,
        provider_name: provider,
        model_name: model,
        api_key,
        base_url: Some(base_url),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A successful send resolves immediately and propagates the result.
    #[tokio::test]
    async fn await_screenshot_result_returns_ok_when_sender_succeeds() {
        let (tx, rx) = std::sync::mpsc::sync_channel::<anyhow::Result<()>>(1);
        tx.send(Ok(())).unwrap();
        let res = await_screenshot_result(rx, Duration::from_secs(5)).await;
        assert!(res.is_ok());
    }

    /// An error sent through the channel is propagated to the caller.
    #[tokio::test]
    async fn await_screenshot_result_propagates_capture_error() {
        let (tx, rx) = std::sync::mpsc::sync_channel::<anyhow::Result<()>>(1);
        tx.send(Err(anyhow::anyhow!("capture failed: boom")))
            .unwrap();
        let res = await_screenshot_result(rx, Duration::from_secs(5)).await;
        let err = res.expect_err("expected capture error");
        assert!(err.to_string().contains("boom"));
    }

    /// If the sender is dropped without sending, we surface a "channel closed" error
    /// instead of hanging.
    #[tokio::test]
    async fn await_screenshot_result_reports_channel_closed_when_sender_dropped() {
        let (tx, rx) = std::sync::mpsc::sync_channel::<anyhow::Result<()>>(1);
        drop(tx);
        let res = await_screenshot_result(rx, Duration::from_secs(5)).await;
        let err = res.expect_err("expected channel-closed error");
        assert!(err.to_string().contains("channel closed"));
    }

    /// If neither sender nor result ever arrives, the timeout fires rather than
    /// blocking forever — the bug fix for #103.
    #[tokio::test]
    async fn await_screenshot_result_times_out_when_sender_stalls() {
        let (tx, rx) = std::sync::mpsc::sync_channel::<anyhow::Result<()>>(1);
        // Keep the sender alive across the await so rx.recv() actually blocks.
        let res = await_screenshot_result(rx, Duration::from_millis(50)).await;
        drop(tx);
        let err = res.expect_err("expected timeout error");
        assert!(err.to_string().contains("timed out"), "got: {}", err);
    }

    /// Regression guard for #337 — lock-order inversion in Tier 2/3 callbacks.
    ///
    /// The documented lock-ordering contract requires `world` to be acquired
    /// before `npc_manager`.  Before the fix, the Tier 2 and Tier 3 callback
    /// tasks acquired them in the opposite order (`npc_manager` first), which
    /// could deadlock against the main tick that holds `world` while awaiting
    /// gossip propagation.
    ///
    /// This test uses two tasks that each hold one of the two locks and then
    /// try to acquire the other, mimicking the pre-fix scenario.  With the
    /// correct ordering (`world` first) in both tasks there is no
    /// circular-wait and both complete without timing out.
    #[tokio::test]
    async fn tier_callback_lock_order_world_before_npc_manager() {
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let world_lock: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));
        let npc_lock: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));

        // Task A: acquires world then npc_manager (correct order — main tick pattern).
        let wl_a = Arc::clone(&world_lock);
        let nl_a = Arc::clone(&npc_lock);
        let task_a = tokio::spawn(async move {
            let mut w = wl_a.lock().await;
            // Yield so task B can start and attempt its own acquire in whatever
            // order it uses.
            tokio::task::yield_now().await;
            let mut n = nl_a.lock().await;
            *w += 1;
            *n += 1;
        });

        // Task B: must also acquire world then npc_manager (fixed order).
        // Pre-fix this was reversed (npc_manager first), causing circular-wait.
        let wl_b = Arc::clone(&world_lock);
        let nl_b = Arc::clone(&npc_lock);
        let task_b = tokio::spawn(async move {
            // world first — matches the corrected Tier 2/3 callbacks.
            let mut w = wl_b.lock().await;
            tokio::task::yield_now().await;
            let mut n = nl_b.lock().await;
            *w += 10;
            *n += 10;
        });

        // Both tasks must complete within a generous timeout.  A deadlock
        // would stall them indefinitely; the select ensures the test fails
        // fast rather than hanging the whole suite.
        let timeout = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            task_a.await.unwrap();
            task_b.await.unwrap();
        });
        assert!(
            timeout.await.is_ok(),
            "tasks deadlocked — lock-order inversion re-introduced"
        );

        // Sanity: both tasks ran and incremented the counters.
        assert_eq!(*world_lock.lock().await, 11);
        assert_eq!(*npc_lock.lock().await, 11);
    }

    #[test]
    fn parse_demo_args_defaults_when_no_flags() {
        let args: Vec<String> = vec!["parish-tauri".to_string()];
        let cfg = super::parse_demo_args(&args);
        assert!(!cfg.auto_start);
        assert!(cfg.extra_prompt.is_none());
        assert!((cfg.turn_pause_secs - 2.0).abs() < f32::EPSILON);
        assert!(cfg.max_turns.is_none());
    }

    #[test]
    fn parse_demo_args_sets_auto_start() {
        let args: Vec<String> = vec!["parish-tauri".to_string(), "--demo".to_string()];
        let cfg = super::parse_demo_args(&args);
        assert!(cfg.auto_start);
    }

    #[test]
    fn parse_demo_args_custom_pause_and_max_turns() {
        let args: Vec<String> = vec![
            "parish-tauri".to_string(),
            "--demo".to_string(),
            "--demo-pause".to_string(),
            "5".to_string(),
            "--demo-max-turns".to_string(),
            "10".to_string(),
        ];
        let cfg = super::parse_demo_args(&args);
        assert!(cfg.auto_start);
        assert!((cfg.turn_pause_secs - 5.0).abs() < f32::EPSILON);
        assert_eq!(cfg.max_turns, Some(10));
    }
}
