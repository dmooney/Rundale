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
    /// Display name (full name if introduced, brief description otherwise).
    pub name: String,
    /// NPC's occupation.
    pub occupation: String,
    /// NPC's current mood.
    pub mood: String,
    /// Whether the player has been introduced to this NPC.
    pub introduced: bool,
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

/// Mutable runtime configuration for provider, model, and cloud settings.
///
/// Wrapped in a `Mutex` on [`AppState`] so `/provider`, `/model`, `/key`,
/// `/cloud`, and `/improv` commands can update it at runtime.
pub struct GameConfig {
    /// Display name of the current base provider (e.g. "ollama", "openrouter").
    pub provider_name: String,
    /// Base URL for the current provider API.
    pub base_url: String,
    /// API key for the current provider (None for keyless providers like Ollama).
    pub api_key: Option<String>,
    /// Model name for NPC dialogue inference.
    pub model_name: String,
    /// Cloud provider name for dialogue (None = local only).
    pub cloud_provider_name: Option<String>,
    /// Cloud model name for dialogue.
    pub cloud_model_name: Option<String>,
    /// Cloud API key.
    pub cloud_api_key: Option<String>,
    /// Cloud base URL.
    pub cloud_base_url: Option<String>,
    /// Whether improv craft mode is enabled for NPC dialogue.
    pub improv_enabled: bool,
    /// Per-category provider name overrides (None = inherits base).
    pub category_provider: [Option<String>; 3],
    /// Per-category model name overrides (None = inherits base).
    pub category_model: [Option<String>; 3],
    /// Per-category API key overrides (None = inherits base).
    pub category_api_key: [Option<String>; 3],
    /// Per-category base URL overrides (None = inherits base).
    pub category_base_url: [Option<String>; 3],
}

impl GameConfig {
    /// Returns the array index for a category (Dialogue=0, Simulation=1, Intent=2).
    fn cat_idx(cat: parish_core::config::InferenceCategory) -> usize {
        use parish_core::config::InferenceCategory;
        match cat {
            InferenceCategory::Dialogue => 0,
            InferenceCategory::Simulation => 1,
            InferenceCategory::Intent => 2,
        }
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
    let screenshot_dir: Option<PathBuf> = {
        let args: Vec<String> = std::env::args().collect();
        args.iter()
            .position(|a| a == "--screenshot")
            .and_then(|i| args.get(i + 1))
            .map(|s| {
                let p = PathBuf::from(s);
                if p.is_absolute() {
                    p
                } else {
                    // src-tauri/ is one level below the workspace root
                    let workspace_root = data_dir
                        .parent()
                        .map(|d| d.to_path_buf())
                        .unwrap_or_else(|| PathBuf::from(".."));
                    workspace_root.join(p)
                }
            })
    };

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
    let (client, model_name, provider_name, base_url, api_key) = build_client_from_env();
    let cloud_env = build_cloud_client_from_env();

    let state = Arc::new(AppState {
        world: Mutex::new(world),
        npc_manager: Mutex::new(npc_manager),
        inference_queue: Mutex::new(None),
        client: Mutex::new(client.clone()),
        cloud_client: Mutex::new(cloud_env.client),
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
            category_provider: [None, None, None],
            category_model: [None, None, None],
            category_api_key: [None, None, None],
            category_base_url: [None, None, None],
        }),
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

                    // Emit an initial theme so the frontend has a palette painted
                    // before the first capture (screenshot mode skips the normal
                    // 500ms theme tick, leaving the WebView on its default white).
                    {
                        use chrono::Timelike;
                        let world = state_ss.world.lock().await;
                        let now = world.clock.now();
                        let raw = compute_palette(
                            now.hour(),
                            now.minute(),
                            world.clock.season(),
                            world.weather,
                        );
                        let _ = handle_ss.emit(events::EVENT_THEME_UPDATE, ThemePalette::from(raw));
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
                            // Push updated theme to frontend
                            let now = world.clock.now();
                            let raw = compute_palette(
                                now.hour(),
                                now.minute(),
                                world.clock.season(),
                                world.weather,
                            );
                            let palette = ThemePalette::from(raw);
                            let _ = handle_ss.emit(events::EVENT_THEME_UPDATE, palette);
                        }

                        // Wait for Svelte to re-render and WebKit to commit the frame
                        tokio::time::sleep(Duration::from_secs(5)).await;

                        // GDK must be called from the GTK main thread; dispatch and await.
                        let path = dir.join(format!("gui-{}.png", name));
                        if let Err(e) = dispatch_screenshot(path).await {
                            eprintln!("screenshot: failed for {name}: {e}");
                        }
                    }

                    println!("screenshot: all done, exiting");
                    std::process::exit(0);
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
                        let _worker = spawn_inference_worker(client.clone(), rx);
                        let queue = InferenceQueue::new(tx);
                        let mut iq = state_setup.inference_queue.lock().await;
                        *iq = Some(queue);
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
                let state_theme = Arc::clone(&state_setup);
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
    let base_url = std::env::var("PARISH_BASE_URL").unwrap_or_else(|_| match provider.as_str() {
        "ollama" => "http://localhost:11434".to_string(),
        "lmstudio" => "http://localhost:1234".to_string(),
        "openrouter" => "https://openrouter.ai/api/v1".to_string(),
        _ => "http://localhost:11434".to_string(),
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
    let base_url = std::env::var("PARISH_CLOUD_BASE_URL")
        .unwrap_or_else(|_| "https://openrouter.ai/api/v1".to_string());
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
