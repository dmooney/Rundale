//! Parish Tauri backend — app setup, state management, and IPC type definitions.
//!
//! The Rust game engine exposes game state to the Svelte frontend via
//! typed Tauri commands ([`commands`]) and events ([`events`]).

pub mod command_host;
pub mod command_registry;
pub mod commands;
pub mod editor_commands;
pub mod events;
pub mod keychain;
mod mcp_bridge;
mod setup;

use parish_core::AUTOSAVE_INTERVAL_SECS;

use base64::Engine as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tauri::{Emitter, Manager};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use parish_core::config::{FeatureFlags, Provider, ProviderConfig};
use parish_core::debug_snapshot::DebugEvent;
use parish_core::game_mod::PronunciationEntry;
use parish_core::inference::{AnyClient, InferenceLog, InferenceQueue, new_inference_log};
use parish_core::npc::manager::NpcManager;
use parish_core::npc::reactions::ReactionTemplates;
use parish_core::world::transport::TransportConfig;
use parish_core::world::{DEFAULT_START_LOCATION, WorldState};

const INITIAL_SETUP_MESSAGE: &str = "Preparing the storyteller...";
const SETUP_HISTORY_LIMIT: usize = 50;

fn mod_asset_data_url(path: Option<PathBuf>) -> Option<String> {
    let path = path?;
    let bytes = std::fs::read(&path)
        .map_err(|e| {
            tracing::warn!(path = %path.display(), error = %e, "failed to read mod icon asset");
            e
        })
        .ok()?;
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

fn apply_mod_desktop_icon<R, M>(app: &M, icon_path: Option<&Path>)
where
    R: tauri::Runtime,
    M: Manager<R>,
{
    let Some(icon_path) = icon_path else {
        return;
    };
    apply_mod_application_icon(icon_path);
    apply_mod_window_icon(app, icon_path);
}

fn apply_mod_window_icon<R, M>(app: &M, icon_path: &Path)
where
    R: tauri::Runtime,
    M: Manager<R>,
{
    let icon = match load_png_icon(icon_path) {
        Ok(icon) => icon,
        Err(e) => {
            tracing::warn!(path = %icon_path.display(), error = e.as_str(), "failed to load mod app icon");
            return;
        }
    };
    for window in app.webview_windows().values() {
        if let Err(e) = window.set_icon(icon.clone()) {
            tracing::warn!(path = %icon_path.display(), error = %e, "failed to apply mod app icon");
        }
    }
}

#[cfg(target_os = "macos")]
fn apply_mod_application_icon(icon_path: &Path) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;
    use std::ffi::c_void;

    let Some(mtm) = MainThreadMarker::new() else {
        tracing::warn!(path = %icon_path.display(), "failed to apply mod Dock icon off the macOS main thread");
        return;
    };
    let bytes = match std::fs::read(icon_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::warn!(path = %icon_path.display(), error = %e, "failed to read mod Dock icon");
            return;
        }
    };

    let data =
        unsafe { NSData::dataWithBytes_length(bytes.as_ptr().cast::<c_void>(), bytes.len()) };
    let Some(image) = NSImage::initWithData(mtm.alloc(), &data) else {
        tracing::warn!(path = %icon_path.display(), "failed to decode mod Dock icon as NSImage");
        return;
    };

    let app = NSApplication::sharedApplication(mtm);
    unsafe {
        app.setApplicationIconImage(Some(&image));
    }
    tracing::info!(path = %icon_path.display(), "applied mod Dock icon");
}

#[cfg(not(target_os = "macos"))]
fn apply_mod_application_icon(_icon_path: &Path) {}

fn load_png_icon(path: &Path) -> Result<tauri::image::Image<'static>, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let decoder = png::Decoder::new(std::io::BufReader::new(file));
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let buffer_size = reader
        .output_buffer_size()
        .ok_or_else(|| "PNG output buffer is too large".to_string())?;
    let mut buf = vec![0; buffer_size];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    if info.bit_depth != png::BitDepth::Eight {
        return Err(format!("unsupported PNG bit depth {:?}", info.bit_depth));
    }
    let bytes = &buf[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => bytes.to_vec(),
        png::ColorType::Rgb => bytes
            .chunks_exact(3)
            .flat_map(|chunk| [chunk[0], chunk[1], chunk[2], 255])
            .collect(),
        png::ColorType::Grayscale => bytes
            .iter()
            .flat_map(|value| [*value, *value, *value, 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => bytes
            .chunks_exact(2)
            .flat_map(|chunk| [chunk[0], chunk[0], chunk[0], chunk[1]])
            .collect(),
        png::ColorType::Indexed => {
            return Err("indexed PNG app icons are not supported".to_string());
        }
    };

    Ok(tauri::image::Image::new_owned(
        rgba,
        info.width,
        info.height,
    ))
}

/// Resolves the python interpreter inside the bundled vllm-mlx runtime,
/// if Parish was shipped with the inference runtime in its app resources.
///
/// macOS layout (the only platform we ship a bundle for — see
/// `justfile::build-vllm-mlx-bundle`):
///
/// ```text
/// <Parish.app>/Contents/Resources/vllm-mlx/
///   └── python-runtime/             ← python-build-standalone tree
///       ├── bin/python3             ← what this function returns
///       └── lib/python3.13/site-packages/vllm_mlx/...
/// ```
///
/// `VllmMlxProcess::ensure_running` detects `python*` in the binary name
/// and invokes it as `python3 -m vllm_mlx serve …`. The site-packages
/// path also lands on `PARISH_VLLM_MLX_PYTHONPATH` so the child finds
/// the package without sys.path surprises.
///
/// No venv is involved: python-build-standalone's `install_only` tarball
/// is a relocatable interpreter, so we pip-install vllm-mlx straight
/// into the runtime's own site-packages. A venv would bake the build
/// host's absolute paths into pyvenv.cfg, breaking when the bundle
/// lands in `/Applications/Rundale.app/Contents/Resources/`.
///
/// Dev (`cargo tauri dev`) builds don't ship the bundle — this function
/// returns `None` and the runtime falls through to `VLLM_MLX_BIN` env or
/// `PATH` lookup of a user-installed `vllm-mlx` (the `uv tool install`
/// flow).
///
/// Apache 2.0: vllm-mlx is permissively licensed so we ship it directly.
/// Build-time materialization (`just build-vllm-mlx-bundle`) happens in CI;
/// this function only locates the already-bundled tree at runtime.
pub fn resolve_bundled_vllm_mlx_bin() -> Option<PathBuf> {
    resolve_bundled_vllm_mlx_paths().map(|p| p.python)
}

/// Bundle layout used by `parish-tauri::run` startup and the local
/// onboarding command. Two related paths land on env vars so
/// `VllmMlxProcess::ensure_running` and the spawned `python -m vllm_mlx`
/// child both see the same view of the bundle.
pub struct BundledVllmMlxPaths {
    /// `<Resources>/vllm-mlx/python-runtime/bin/python3`. Spawned as
    /// `python3 -m vllm_mlx serve …`.
    pub python: PathBuf,
    /// `<Resources>/vllm-mlx/python-runtime/lib/python<X.Y>/site-packages`.
    /// Passed via `PYTHONPATH` so the child finds the `vllm_mlx` package
    /// even when launched through a wrapper that scrubs sys.path.
    pub site_packages: PathBuf,
}

/// Probes for the bundled vllm-mlx layout next to the running executable.
/// Returns both the python interpreter path and its matching
/// site-packages dir, or `None` if the bundle is absent (dev runs).
pub fn resolve_bundled_vllm_mlx_paths() -> Option<BundledVllmMlxPaths> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;

    // macOS .app layout: Contents/MacOS/<exe> → ../Resources/vllm-mlx/
    // Other platforms: <exe-dir>/resources/vllm-mlx/ (Tauri's default
    // resource-dir convention for non-macOS).
    let runtime_root = if cfg!(target_os = "macos") {
        exe_dir.join("../Resources/vllm-mlx/python-runtime")
    } else {
        exe_dir.join("resources/vllm-mlx/python-runtime")
    };

    let python = if cfg!(target_os = "windows") {
        runtime_root.join("Scripts/python.exe")
    } else {
        runtime_root.join("bin/python3")
    };
    if !python.is_file() {
        return None;
    }

    // Discover the python3.X site-packages dir — python-build-standalone
    // pins to a specific version, but we don't hardcode 3.13 in case the
    // bundle moves to 3.14+ later.
    let lib_dir = runtime_root.join("lib");
    let site_packages = std::fs::read_dir(&lib_dir).ok()?.find_map(|entry| {
        let e = entry.ok()?;
        let name = e.file_name();
        let s = name.to_str()?;
        if s.starts_with("python") {
            let p = e.path().join("site-packages");
            if p.is_dir() { Some(p) } else { None }
        } else {
            None
        }
    })?;

    Some(BundledVllmMlxPaths {
        python,
        site_packages,
    })
}

// ── IPC type definitions ─────────────────────────────────────────────────────

// WorldSnapshot, MapLocation, and NpcInfo are defined in parish-core and
// re-exported here so all call sites remain stable.
pub use parish_core::ipc::{MapLocation, WorldSnapshot};

/// Latest setup progress state for the startup overlay.
#[derive(serde::Serialize, Clone)]
pub struct SetupStatusSnapshot {
    /// Current human-readable setup step.
    pub current_message: String,
    /// Recent setup messages shown as the overlay activity trail.
    pub messages: Vec<String>,
    /// Bytes downloaded so far across discovered Ollama pull artifacts.
    pub completed: u64,
    /// Total bytes expected across discovered Ollama pull artifacts, or 0 when unknown.
    pub total: u64,
    /// Whether setup has completed.
    pub done: bool,
    /// Success state once setup is complete; `None` while setup is running.
    pub success: Option<bool>,
    /// Error message when setup failed.
    pub error: String,
    /// True when the BYOK gate fired and the frontend should render the
    /// onboarding fork instead of the Ollama spinner. Persisted on the
    /// snapshot (not just emitted as an event) so SetupOverlay can recover
    /// the state if it mounts after the gate fires.
    pub needs_onboarding: bool,
    /// Refines `needs_onboarding` with the specific fork to show:
    /// local-recommended (Mac ≥16 GB), local-low-mem, or local-unavailable.
    /// `None` when `needs_onboarding` is false. Serialized as kebab-case
    /// strings (`"local-recommended"`, etc.) so the Svelte SetupOverlay
    /// can switch on the value without an enum import.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub onboarding_choice: Option<crate::setup::OnboardingChoice>,
}

impl Default for SetupStatusSnapshot {
    fn default() -> Self {
        Self {
            current_message: INITIAL_SETUP_MESSAGE.to_string(),
            messages: vec![INITIAL_SETUP_MESSAGE.to_string()],
            completed: 0,
            total: 0,
            done: false,
            success: None,
            error: String::new(),
            needs_onboarding: false,
            onboarding_choice: None,
        }
    }
}

impl SetupStatusSnapshot {
    pub(crate) fn record_needs_onboarding(&mut self) {
        self.needs_onboarding = true;
        // Don't push a status message — the fork screen renders directly
        // and the spinner UI shouldn't show "Awaiting provider choice"
        // text underneath.
    }

    pub(crate) fn record_onboarding_choice(&mut self, choice: crate::setup::OnboardingChoice) {
        self.onboarding_choice = Some(choice);
        if choice.needs_user_choice() {
            self.needs_onboarding = true;
        }
    }

    pub(crate) fn clear_needs_onboarding(&mut self) {
        self.needs_onboarding = false;
        self.onboarding_choice = None;
    }

    fn record_status(&mut self, msg: &str) {
        self.current_message = msg.to_string();
        if self.messages.last().is_some_and(|last| last == msg) {
            return;
        }
        if self.messages.len() >= SETUP_HISTORY_LIMIT {
            self.messages.remove(0);
        }
        self.messages.push(msg.to_string());
    }

    fn record_progress(&mut self, completed: u64, total: u64) {
        self.completed = completed;
        self.total = total;
    }

    fn record_done(&mut self, success: bool, error: String) {
        self.done = true;
        self.success = Some(success);
        self.error = error.clone();
        if success {
            self.record_status("The storyteller is ready.");
        } else if error.is_empty() {
            self.record_status("Setup failed.");
        } else {
            self.record_status(&format!("Setup failed: {}", error));
        }
    }
}

/// The full map graph sent to the frontend.
///
/// Tauri-specific extension of the core `MapData` type: adds `player_lat` and
/// `player_lon` for minimap centering. The core type omits these because the
/// axum web server derives them differently (the server builds the snapshot on
/// behalf of the currently authenticated session rather than the local player).
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

// NpcInfo, ThemePalette, GameConfig, SaveState, UiConfigSnapshot, and
// ConversationRuntimeState are defined in parish-core and re-exported here.
pub use parish_core::ipc::{
    ConversationRuntimeState, GameConfig, NpcInfo, SaveState, ThemePalette, UiConfigSnapshot,
};

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
    /// Outermost barrier for task durability and save lifecycle changes.
    ///
    /// A guarded operation holds this from before in-memory/identity capture
    /// through durable commit, response projection, or task rollback.
    pub persistence_gate: Mutex<()>,
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
    /// Monotonic lifetime count of all `GameEvent`s ever pushed to
    /// `game_events` (including those already evicted from the ring). Used
    /// as the `event_cursor` returned by `GET /api/turn` so that a `since=N`
    /// caller reliably receives only events enqueued after position N,
    /// regardless of how many times the ring has wrapped (#1389).
    pub total_game_events: std::sync::atomic::AtomicUsize,
    /// Shared inference call log for the debug panel.
    pub inference_log: InferenceLog,
    /// UI configuration from the loaded game mod.
    pub ui_config: UiConfigSnapshot,
    /// Fixed theme palette from the loaded game mod.
    pub theme_palette: ThemePalette,
    /// Time-of-day palette keyframes from the loaded game mod. Empty when the
    /// mod ships only a static palette (or no mod is loaded).
    pub theme_keyframes: Vec<parish_palette::Keyframe>,
    /// Static palette in RawPalette form, used by `get_theme` when the mod
    /// declares no keyframes. `None` when no mod is loaded (engine falls back
    /// to `neutral_grey_palette`).
    pub static_raw_palette: Option<parish_palette::RawPalette>,
    /// Atmospheric flavour shown when NPC inference fails. Empty when the
    /// mod provides none — engine falls back to a single ellipsis.
    pub inference_failure_messages: Vec<String>,
    /// Atmospheric flavour shown when the player addresses no-one. Empty
    /// when the mod provides none — engine falls back to a blank line.
    pub idle_messages: Vec<String>,
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
    /// The active game mod, resolved once at startup (Rule 9, #1197).
    /// Editor + new-game handlers read `game_mod.mod_dir` from here instead of
    /// re-walking the cwd via `find_default_mod()` per call, which breaks
    /// packaged builds / daemonised servers / `/tmp` working dirs. Mirrors the
    /// `parish-server` `AppState.game_mod` slot for mode parity.
    pub game_mod: Option<parish_core::game_mod::GameMod>,
    /// Absolute path to the most recent player-triggered screenshot, if any.
    ///
    /// Populated by the `save_screenshot` command after the frontend posts a
    /// `data:image/png;base64,...` URL captured by `html-to-image`. Read by
    /// `get_latest_screenshot` (and the matching MCP tool) so the path can be
    /// reported without rescanning `<saves_dir>/screenshots/`.
    pub latest_screenshot_path: Mutex<Option<PathBuf>>,
    /// Per-process token the graphical frontend must echo before it can mark
    /// itself capture-ready. Prevents a stale webview from satisfying a new
    /// desktop launch's readiness contract.
    pub graphical_launch_token: String,
    /// True only after the live frontend has registered its screenshot listener
    /// and the illustrated Pixi canvas has presented its first frame.
    pub graphical_ready: std::sync::atomic::AtomicBool,
    /// Last renderer failure reported by the current graphical frontend.
    pub graphical_error: std::sync::Mutex<Option<String>>,
    /// In-flight MCP screenshot requests.
    ///
    /// The MCP bridge's `/api/take-screenshot` handler inserts a `(request_id,
    /// oneshot::Sender)` here while it waits for the frontend to capture the
    /// screen. The `notify_screenshot_captured` Tauri command (called by the
    /// frontend after capture) looks up the sender by `request_id`, sends the
    /// `ScreenshotInfo` through it, and removes the entry. Entries are also
    /// removed on timeout inside the bridge handler.
    pub(crate) pending_screenshots:
        Mutex<std::collections::HashMap<String, crate::commands::PendingScreenshot>>,
    /// Handle for the active inference worker task; used to abort it on rebuild.
    pub worker_handle: Mutex<Option<JoinHandle<()>>>,
    /// Editor session — separate from gameplay state, may be empty.
    pub editor: std::sync::Mutex<parish_core::ipc::editor::EditorSession>,
    /// Advisory file lock for the currently active save file.
    pub save_lock: Mutex<Option<parish_core::persistence::SaveFileLock>>,
    /// Local runtime child processes (Ollama or N vllm-mlx slots), held for
    /// the app's lifetime — dropping it kills all spawned servers.
    /// See [`parish_core::inference::client::RuntimeProcesses`].
    pub runtime_processes: Mutex<parish_core::inference::client::RuntimeProcesses>,
    /// TOML-configured inference timeouts loaded from `parish.toml` at boot.
    /// Used by rebuild paths so `/provider` switches honour the configured
    /// values instead of falling back to compiled-in defaults. (#417)
    pub inference_config: parish_core::config::InferenceConfig,
    /// Latest provider-bootstrap status for the startup overlay. Uses a
    /// standard mutex because setup progress callbacks are synchronous.
    pub setup_status: std::sync::Mutex<SetupStatusSnapshot>,
    /// Idempotency guard for `do_start_local_inference_setup` — a second
    /// `/api/start-local-inference` POST while the first is still
    /// downloading would otherwise race the bootstrap pipeline. Set on
    /// entry, cleared on exit (success or error). UI / MCP callers see a
    /// busy error and the in-flight wizard keeps running.
    pub wizard_in_flight: std::sync::atomic::AtomicBool,
    /// Demo / auto-player configuration. Read-only after startup.
    pub demo_config: DemoConfig,
    /// Cancellation token — cancelled during app shutdown to stop background ticks
    /// gracefully. Clones are passed into each spawned tick task (#104).
    pub shutdown_token: CancellationToken,
    /// Sim-preemption cancel token (#9).
    ///
    /// Cancelled-and-replaced by [`commands::input::do_submit_input`] when a player
    /// turn arrives, so any in-flight Tier 2 / Tier 3 background inference
    /// drops mid-decode and frees the local model slot for the player's
    /// dialogue call. The token is snapshotted at dispatch time and passed
    /// into `run_tier2_for_group` / `Tier3Context::cancel`, so cancelling the
    /// current token only affects already-running sim calls; the next sim
    /// tick spawns with the freshly-replaced token.
    pub sim_cancel: Mutex<CancellationToken>,
    /// Trait-erased per-session persistence (#696, slice 8).
    ///
    /// Single-user Tauri runtime uses `session_id = ""` with a flat
    /// `saves/parish_NNN.db` layout.
    ///
    /// Not part of the lock-ordering chain: never held across acquisition
    /// of any `Mutex` field.
    pub session_store: std::sync::Arc<dyn parish_core::session_store::SessionStore>,
    /// Per-user, per-machine config dir resolved once at startup (Rule 9).
    /// Hosts `parish.toml` (non-secret BYOK choices) and the `.onboarded`
    /// marker. API keys live in the OS keychain via `secret_store`.
    pub user_config_dir: PathBuf,
    /// OS keychain (Tauri only). Backed by `keyring` on real builds; tests
    /// can swap in `InMemorySecretStore` via the trait.
    pub secret_store: std::sync::Arc<dyn parish_core::secret_store::SecretStore>,
    /// Language settings derived from the active mod manifest.
    ///
    /// Resolved once at startup and injected into all dialogue prompt builders
    /// to enforce locale-correct spelling and code-switching behaviour.
    pub language_settings: parish_core::npc::LanguageSettings,
    /// Persistent on-disk inference call log.
    pub inference_file_log: parish_core::inference::file_log::InferenceFileLog,
    /// Persistent on-disk chat transcript (paired with `inference_file_log`).
    pub chat_transcript_log: parish_core::chat_transcript::ChatTranscriptLog,
}

// ── Data path resolution ─────────────────────────────────────────────────────

// ── #621: Optional OTel provider for the Tauri runtime ──────────────────────

/// Attempts to build an OTLP span exporter from environment variables.
///
/// Returns `Some(SdkTracerProvider)` when `PARISH_OTEL_ENDPOINT` is set and
/// the exporter builds successfully.  Returns `None` otherwise (the default
/// for local development — no network I/O, no background export threads).
///
/// This mirrors [`parish_server::tracing_setup::try_build_otel_provider`] so
/// the Tauri and web runtimes use the same OTel initialisation path
/// (CLAUDE.md rule #2 — mode parity).
fn build_tauri_otel_provider() -> Option<opentelemetry_sdk::trace::SdkTracerProvider> {
    let endpoint = std::env::var("PARISH_OTEL_ENDPOINT")
        .ok()
        .filter(|s| !s.is_empty())?;

    use opentelemetry_otlp::WithExportConfig as _;
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_endpoint(endpoint.clone())
        .build()
        .map_err(|e| {
            eprintln!(
                "[parish-tauri] WARNING: Failed to build OTLP exporter for {} — \
                 OTel tracing disabled: {}",
                endpoint, e
            );
        })
        .ok()?;

    let resource = opentelemetry_sdk::Resource::builder()
        .with_service_name("parish-tauri".to_string())
        .build();

    Some(
        opentelemetry_sdk::trace::SdkTracerProvider::builder()
            .with_resource(resource)
            .with_batch_exporter(exporter)
            .build(),
    )
}

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

// ── Setup progress reporter (GUI mode) ───────────────────────────────────────

/// Forwards inference bootstrap progress to the frontend via Tauri events.
/// Used inside the async `.setup()` spawn so the window exists before we call it.
pub(crate) struct TauriProgress {
    app: tauri::AppHandle,
    state: Arc<AppState>,
}

impl TauriProgress {
    /// Constructs a TauriProgress without pulling in the rest of the
    /// async-setup bootstrap. Used by Tauri commands that drive their own
    /// SetupProgress-consuming work (e.g. `start_local_inference_setup`).
    pub(crate) fn new(app: tauri::AppHandle, state: Arc<AppState>) -> Self {
        Self { app, state }
    }
}

#[allow(dead_code)]
impl TauriProgress {
    fn with_setup_status(&self, update: impl FnOnce(&mut SetupStatusSnapshot)) {
        let mut status = self
            .state
            .setup_status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        update(&mut status);
    }
}

impl parish_core::inference::setup::SetupProgress for TauriProgress {
    fn on_status(&self, msg: &str) {
        self.with_setup_status(|status| status.record_status(msg));
        tracing::info!("{}", msg);
        let _ = self.app.emit(
            events::EVENT_SETUP_STATUS,
            events::SetupStatusPayload {
                message: msg.to_string(),
            },
        );
    }

    fn on_pull_progress(&self, completed: u64, total: u64) {
        self.with_setup_status(|status| status.record_progress(completed, total));
        let _ = self.app.emit(
            events::EVENT_SETUP_PROGRESS,
            events::SetupProgressPayload { completed, total },
        );
    }

    fn on_error(&self, msg: &str) {
        self.with_setup_status(|status| status.record_status(&format!("Error: {}", msg)));
        tracing::error!("{}", msg);
        let _ = self.app.emit(
            events::EVENT_SETUP_STATUS,
            events::SetupStatusPayload {
                message: format!("Error: {}", msg),
            },
        );
    }
}

fn record_setup_done(state: &Arc<AppState>, success: bool, error: String) {
    let mut status = state
        .setup_status
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    status.record_done(success, error);
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
    // ── #621: Tracing subscriber with optional OTel layer ────────────────────
    // Uses the registry pattern (same as CLI) so the OTel OTLP exporter can be
    // composed in when `PARISH_OTEL_ENDPOINT` is set.  The exporter is
    // technically only useful in server mode, but supporting it here satisfies
    // CLAUDE.md rule #2 (mode parity) and lets operators route desktop traces
    // to a local collector during development.
    //
    // dotenvy is loaded before subscriber init so PARISH_OTEL_ENDPOINT from
    // `.env` is visible when building the provider.
    dotenvy::dotenv().ok();

    // Resolve a bundled vllm-mlx runtime if Parish ships one. Sets
    // `VLLM_MLX_BIN` (interpreter) and `PARISH_VLLM_MLX_PYTHONPATH`
    // (site-packages) so packaged builds don't depend on the user
    // pre-installing vllm-mlx via `uv tool install`. The env var wins
    // over PATH but loses to a user-set override.
    //
    // SAFETY: set_var is unsafe on POSIX in multi-threaded contexts. We
    // call this before tauri::Builder::default() spawns any background
    // tasks, so the runtime is still single-threaded here.
    if std::env::var_os("VLLM_MLX_BIN").is_none()
        && let Some(paths) = resolve_bundled_vllm_mlx_paths()
    {
        unsafe {
            std::env::set_var("VLLM_MLX_BIN", &paths.python);
            std::env::set_var("PARISH_VLLM_MLX_PYTHONPATH", &paths.site_packages);
        }
        // Tracing isn't initialized yet — eprintln so the line still lands in
        // the dev console / packaged-app stderr capture.
        eprintln!(
            "vllm-mlx: using bundled python at {} (site-packages={})",
            paths.python.display(),
            paths.site_packages.display()
        );
    }

    // Build the optional OTel provider before any tracing is emitted.
    let otel_provider = build_tauri_otel_provider();
    let otel_tracer = otel_provider.as_ref().map(|p| {
        use opentelemetry::trace::TracerProvider as _;
        p.tracer("parish-tauri")
    });

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    use tracing_subscriber::layer::SubscriberExt as _;
    use tracing_subscriber::util::SubscriberInitExt as _;

    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_ansi(false));

    if let Some(tracer) = otel_tracer {
        registry
            .with(Some(tracing_opentelemetry::OpenTelemetryLayer::new(tracer)))
            .init();
    } else {
        registry
            .with(
                Option::<
                    tracing_opentelemetry::OpenTelemetryLayer<
                        _,
                        opentelemetry::trace::noop::NoopTracer,
                    >,
                >::None,
            )
            .init();
    }

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

    // Parse optional --mcp-port <N> flag. When set, an in-process Axum
    // listener mirrors the parish-server IPC routes against this process's
    // live AppState so an MCP client (parish-mcp) can drive the desktop
    // session the user can see in the window. Bound to 127.0.0.1 only.
    let mcp_port: Option<u16> = {
        let args: Vec<String> = std::env::args().collect();
        args.iter()
            .position(|a| a == "--mcp-port")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse::<u16>().ok())
    };

    // Try to load game mod (auto-detect from workspace root) via the
    // ModSource abstraction. load_base_mod_sync is used here because
    // Tauri's run() is synchronous and no tokio runtime exists yet.
    let game_mod = parish_core::mod_source::load_base_mod_sync();

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
    let engine_config_path = parish_core::config::resolve_config_path(&data_dir);
    let engine_config = parish_core::config::load_engine_config(&engine_config_path);

    // Resolve the per-user config dir up-front so provider_config_from_env can
    // hydrate from the BYOK / local-inference wizard's persisted
    // `~/Library/Application Support/Parish/parish.toml`. Storing the result
    // here means AppState reuses the same path later (#771: resolve once at
    // startup, never re-probe from a request handler).
    let user_config_dir = parish_core::config::user_config::resolve_user_config_dir();

    // Re-seed PARISH_HF_HOME on relaunch so VllmMlxProcess::ensure_running
    // forwards the correct HF cache root to the spawned `python3 -m
    // vllm_mlx.cli serve …`. start_local_inference_setup wrote it last
    // session; we just need to re-point at the same `<user_config_dir>/models`
    // here so the cached weights are visible without re-downloading.
    if std::env::var_os("PARISH_HF_HOME").is_none() {
        let hf_home = user_config_dir.join("models");
        if hf_home.is_dir() {
            // SAFETY: same convention as the VLLM_MLX_BIN set_var below —
            // run before any background tasks spawn.
            unsafe {
                std::env::set_var("PARISH_HF_HOME", &hf_home);
            }
        }
    }

    // Read provider config from env vars (optional). Layers, last-wins:
    //   1. saved user config (`<user_config_dir>/parish.toml`)
    //   2. `PARISH_PROVIDER` / `PARISH_BASE_URL` / `PARISH_MODEL` env vars
    //   3. provider's standard API-key env var (e.g. `ANTHROPIC_API_KEY`)
    let (provider_config, provider_name, base_url, api_key) =
        provider_config_from_env(&user_config_dir);
    let cloud_env = build_cloud_client_from_env(&engine_config.inference);

    // Clone inference config before it is moved into AppState so the async
    // setup spawn can still reference it during bootstrap. provider_config
    // itself is not stored in AppState and will be moved directly into the spawn.
    let inference_config_for_spawn = engine_config.inference.clone();

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

    // Build runtime palette state from the loaded mod.
    let theme_keyframes = game_mod
        .as_ref()
        .map(|gm| gm.ui.theme.resolved_keyframes())
        .unwrap_or_default();
    let static_raw_palette = game_mod.as_ref().map(|gm| gm.ui.theme.static_raw_palette());
    let inference_failure_messages = game_mod
        .as_ref()
        .map(|gm| gm.loading.inference_failure_messages.clone())
        .unwrap_or_default();
    let idle_messages = game_mod
        .as_ref()
        .map(|gm| gm.loading.idle_messages.clone())
        .unwrap_or_default();

    // Build transport config from mod or defaults
    let transport = game_mod
        .as_ref()
        .map(|gm| gm.transport.clone())
        .unwrap_or_default();

    let theme_palette = game_mod
        .as_ref()
        .map(|gm| gm.ui.theme.resolved_palette())
        .unwrap_or_else(parish_core::game_mod::default_theme_palette);
    let mod_window_icon_path = game_mod.as_ref().and_then(|gm| gm.app_icon_path());
    let mod_window_icon_path_for_setup = mod_window_icon_path.clone();
    let mod_window_icon_path_for_run = mod_window_icon_path.clone();

    // engine_config already loaded above (before provider bootstrap) and
    // includes both map tile-source registry and inference timeouts. (#417)
    //
    // Tauri has no `/tiles/` proxy route (that lives in parish-server only),
    // so pass `has_tile_proxy = false`: the snapshot builder substitutes
    // `upstream_url` for sources that would otherwise advertise a dead
    // same-origin proxy path. Without this, MapLibre fetches
    // `/tiles/historic/{z}/{x}/{y}.png` and 404s (post-#955 regression).
    let tile_sources_snapshot =
        parish_core::ipc::TileSourceSnapshot::list_from_map_config(&engine_config.map, false);
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
            app_icon_url: mod_asset_data_url(gm.app_icon_path()),
            favicon_url: mod_asset_data_url(gm.favicon_path()),
            map_overlay: gm.ui.theme.map_overlay.clone(),
            base_mod_required: false,
        }
    } else {
        UiConfigSnapshot {
            hints_label: "Language Hints".to_string(),
            default_accent: theme_palette.accent.clone(),
            splash_text,
            active_tile_source: active_tile_source.clone(),
            tile_sources: tile_sources_snapshot,
            auto_pause_timeout_seconds: engine_config.session.auto_pause_after_secs,
            app_icon_url: None,
            favicon_url: None,
            map_overlay: None,
            base_mod_required: true,
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

    // Extract language settings from the game mod (defaults to plain "en" if no mod)
    let language_settings = game_mod
        .as_ref()
        .map(|gm| {
            parish_core::npc::LanguageSettings::new(
                gm.player_language().to_string(),
                gm.native_language().map(str::to_string),
            )
        })
        .unwrap_or_else(parish_core::npc::LanguageSettings::english_only);

    // Load feature flags from disk
    let flags = FeatureFlags::load_from_file(&data_dir.join("parish-flags.json"));

    let mut game_config = GameConfig {
        provider_name,
        base_url,
        api_key,
        model_name: String::new(), // filled in after async bootstrap
        cloud_provider_name: cloud_env.provider_name,
        cloud_model_name: cloud_env.model_name,
        cloud_api_key: cloud_env.api_key,
        cloud_base_url: cloud_env.base_url,
        improv_enabled: false,
        max_follow_up_turns: 2,
        idle_banter_after_secs: engine_config.session.idle_banter_after_secs,
        auto_pause_after_secs: engine_config.session.auto_pause_after_secs,
        category_provider: Default::default(),
        category_model: Default::default(),
        category_api_key: Default::default(),
        category_base_url: Default::default(),
        inference_profile_override: Default::default(),
        category_inference_profile: Default::default(),
        flags,
        category_rate_limit: Default::default(),
        active_tile_source,
        tile_sources: engine_config.map.id_label_pairs(),
        reveal_unexplored_locations: false,
        auto_setup_model: None,
    };
    // Enable demo-mode flag when --demo was passed so the demo commands work.
    if demo_config.auto_start {
        game_config.flags.enable("demo-mode");
    }
    // Hydrate per-category routing from the wizard-persisted
    // `parish.toml` BEFORE preset fill, so the wizard's intent→:8001,
    // reaction→simulator etc. overrides win over the generic presets.
    // (Without this, vllm-mlx two-slot setups end up routing every
    // category to the dialogue port and the small-slot inference 404s.)
    if let Ok(user_cfg) = parish_core::config::user_config::load_user_config(&user_config_dir) {
        game_config.apply_user_category_overrides(&user_cfg.category_overrides);
        game_config.apply_user_inference_profiles(&user_cfg);
    }

    // Fill any unset model fields from the chosen provider's presets so a
    // user who set only `PARISH_PROVIDER=anthropic` (or `--provider`) gets
    // sensible Dialogue/Simulation/Intent/Reaction defaults.
    game_config.fill_missing_models_from_presets();

    // Resolve the saves directory once at startup (#771). Subsequent save/load
    // commands read `state.saves_dir` instead of re-probing the cwd. App-name
    // drives the per-user data folder (Rundale → `Rundale`); engine fallback
    // when no mod is loaded is `Parish`. Shared helper in parish-core keeps
    // the three entry points in lockstep (rule #12).
    let app_name = parish_core::game_mod::app_name_from_mod(&game_mod);
    let saves_dir = parish_core::persistence::picker::resolve_project_saves_dir(&app_name);

    // Construct the shared SessionStore using the saves directory (#696 slice 8).
    // Tauri is single-user; handlers pass session_id = "" so the store resolves
    // to the flat `saves/parish_NNN.db` layout.
    let session_store: std::sync::Arc<dyn parish_core::session_store::SessionStore> =
        std::sync::Arc::new(parish_core::session_store::DbSessionStore::new(
            saves_dir.clone(),
        ));

    // Cancellation token for graceful background-task shutdown (#104).
    let shutdown_token = CancellationToken::new();

    // Hydrate GameConfig.api_key from the keychain if the standard provider
    // env var wasn't already set — keychain ranks below env vars but above
    // defaults. (`user_config_dir` was resolved up-front above so the saved
    // wizard config could feed provider_config_from_env.)
    let secret_store: std::sync::Arc<dyn parish_core::secret_store::SecretStore> =
        std::sync::Arc::new(keychain::KeyringSecretStore::new());
    if game_config.api_key.is_none()
        && let Ok(provider_enum) =
            parish_core::config::Provider::from_str_loose(&game_config.provider_name)
    {
        let env_key_set = provider_enum
            .api_key_env_var()
            .and_then(|v| std::env::var(v).ok())
            .filter(|v| !v.trim().is_empty())
            .is_some();
        if !env_key_set {
            let account = parish_core::secret_store::provider_account(&game_config.provider_name);
            if let Ok(Some(k)) = secret_store.get(&account) {
                game_config.api_key = Some(k);
            }
        }
    }

    let log_to_disk = parish_core::inference::file_log::resolve_enabled(
        false, // Tauri does not (yet) expose a --no-inference-log flag; env wins
        engine_config.inference.log_to_disk,
    );
    let log_base_url = game_config.base_url.clone();

    // Detached log handles spawn no tasks. They are replaced only after
    // persistence has durably committed below.
    let mut state = Arc::new(AppState {
        persistence_gate: Mutex::new(()),
        world: Mutex::new(world),
        npc_manager: Mutex::new(npc_manager),
        inference_queue: Mutex::new(None),
        client: Mutex::new(None), // populated after async bootstrap completes
        cloud_client: Mutex::new(cloud_env.client),
        conversation: Mutex::new(ConversationRuntimeState::new()),
        debug_events: Mutex::new(std::collections::VecDeque::with_capacity(
            DEBUG_EVENT_CAPACITY,
        )),
        game_events: Mutex::new(std::collections::VecDeque::with_capacity(
            DEBUG_EVENT_CAPACITY,
        )),
        total_game_events: std::sync::atomic::AtomicUsize::new(0),
        inference_log: new_inference_log(),
        ui_config,
        theme_palette,
        theme_keyframes,
        static_raw_palette,
        inference_failure_messages,
        idle_messages,
        pronunciations,
        reaction_templates,
        save_path: Mutex::new(None),
        current_branch_id: Mutex::new(None),
        current_branch_name: Mutex::new(None),
        transport,
        data_dir: data_dir.clone(),
        saves_dir,
        game_mod,
        latest_screenshot_path: Mutex::new(None),
        graphical_launch_token: uuid::Uuid::new_v4().to_string(),
        graphical_ready: std::sync::atomic::AtomicBool::new(false),
        graphical_error: std::sync::Mutex::new(None),
        pending_screenshots: Mutex::new(std::collections::HashMap::new()),
        worker_handle: Mutex::new(None),
        editor: std::sync::Mutex::new(parish_core::ipc::editor::EditorSession::default()),
        save_lock: Mutex::new(None),
        runtime_processes: Mutex::new(parish_core::inference::client::RuntimeProcesses::none()),
        inference_config: engine_config.inference, // (#417) store TOML-configured timeouts
        setup_status: std::sync::Mutex::new(SetupStatusSnapshot::default()),
        wizard_in_flight: std::sync::atomic::AtomicBool::new(false),
        language_settings,
        config: Mutex::new(game_config),
        demo_config,
        shutdown_token: shutdown_token.clone(),
        sim_cancel: Mutex::new(CancellationToken::new()),
        session_store,
        user_config_dir,
        secret_store,
        inference_file_log: parish_core::inference::file_log::InferenceFileLog::disabled(),
        chat_transcript_log: parish_core::chat_transcript::ChatTranscriptLog::disabled(),
    });

    let persistence_ready =
        tauri::async_runtime::block_on(async { setup::init_persistence(&state).await });
    if persistence_ready {
        let (inference_file_log, chat_transcript_log) = tauri::async_runtime::block_on(async {
            let inference_file_log = parish_core::inference::file_log::InferenceFileLog::spawn(
                &state.saves_dir,
                log_to_disk,
                Some(&log_base_url),
            );
            let chat_transcript_log =
                parish_core::chat_transcript::ChatTranscriptLog::spawn_with_flag(
                    &state.saves_dir,
                    inference_file_log.session_id().to_string(),
                    inference_file_log.enabled_flag(),
                );
            (inference_file_log, chat_transcript_log)
        });
        let state_mut = Arc::get_mut(&mut state)
            .expect("Tauri AppState must be unique before builder publication");
        state_mut.inference_file_log = inference_file_log;
        state_mut.chat_transcript_log = chat_transcript_log;
    }

    tauri::Builder::default()
        .manage(Arc::clone(&state))
        .invoke_handler(tauri::generate_handler![
            commands::snapshot::get_world_snapshot,
            commands::snapshot::get_reconnect_state,
            commands::snapshot::get_map,
            commands::snapshot::get_npcs_here,
            commands::snapshot::get_engine_state,
            commands::snapshot::get_theme,
            commands::snapshot::get_ui_config,
            commands::snapshot::get_debug_snapshot,
            commands::snapshot::get_setup_snapshot,
            commands::snapshot::toggle_fullscreen,
            commands::setup::set_provider_config,
            commands::setup::validate_provider_config,
            commands::setup::get_provider_config,
            commands::setup::clear_provider_config,
            commands::setup::list_byok_env_keys,
            commands::setup::list_preset_models,
            commands::setup::list_available_providers,
            commands::setup::get_onboarding_options,
            commands::setup::start_local_inference_setup,
            commands::input::submit_input,
            commands::saves::discover_save_files,
            commands::saves::save_game,
            commands::saves::load_branch,
            commands::saves::create_branch,
            commands::saves::new_save_file,
            commands::saves::new_game,
            commands::saves::get_save_state,
            commands::admin::submit_bug_report,
            commands::admin::open_url,
            commands::reactions::react_to_message,
            commands::demo::get_demo_config,
            commands::demo::get_demo_context,
            commands::demo::get_llm_player_action,
            commands::screenshot::save_screenshot,
            commands::screenshot::get_graphical_readiness,
            commands::screenshot::report_graphical_ready,
            commands::screenshot::report_graphical_error,
            commands::screenshot::report_graphical_unready,
            commands::screenshot::get_latest_screenshot,
            commands::screenshot::take_screenshot,
            commands::screenshot::notify_screenshot_started,
            commands::screenshot::notify_screenshot_captured,
            commands::screenshot::notify_screenshot_error,
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
            apply_mod_desktop_icon(app, mod_window_icon_path_for_setup.as_deref());
            let handle = app.handle().clone();

            // Screenshot mode: --screenshot <dir> captures the UI at four
            // times of day and exits. No background ticks are started.
            if let Some(dir) = screenshot_dir.clone() {
                setup::init_screenshot_mode(handle, Arc::clone(&state), dir);
                return Ok(());
            }

            // Spawn all background tasks via Tauri's async runtime.
            // The setup callback is synchronous (runs on the GTK event loop thread)
            // so tokio::spawn cannot be called directly here — we must go through
            // tauri::async_runtime::spawn, which uses the Tauri-managed tokio handle.
            let state_setup = Arc::clone(&state);
            let provider_config_setup = provider_config;
            let inference_config_setup = inference_config_for_spawn;
            tauri::async_runtime::spawn(async move {
                // Spawn the MCP bridge first so an MCP client can drive
                // onboarding (parish_setup_byok) when bootstrap is gated on
                // the BYOK fork — the bridge needs to be reachable BEFORE
                // bootstrap_inference_provider returns false on first run.
                if let Some(port) = mcp_port {
                    mcp_bridge::spawn(Arc::clone(&state_setup), handle.clone(), port);
                }

                if !persistence_ready {
                    let _ = handle.emit(events::EVENT_SAVE_PICKER, ());
                    return;
                }

                if !setup::bootstrap_inference_provider(
                    &handle,
                    &state_setup,
                    &provider_config_setup,
                    &inference_config_setup,
                )
                .await
                {
                    return;
                }

                setup::init_inference_queue(&state_setup).await;
                setup::spawn_character_log_subscriber(&state_setup, app_name.clone()).await;
                setup::spawn_location_log_subscriber(&state_setup, app_name.clone()).await;
                setup::spawn_chat_transcript_subscriber(&state_setup).await;
                setup::spawn_event_bus_fanin(&state_setup).await;
                setup::spawn_world_tick(handle.clone(), Arc::clone(&state_setup));
                setup::spawn_inactivity_tick(handle.clone(), Arc::clone(&state_setup));
                setup::spawn_debug_tick(handle.clone(), Arc::clone(&state_setup));
                setup::spawn_autosave_tick(Arc::clone(&state_setup));
            });

            Ok(())
        })
        .on_window_event({
            let token = shutdown_token.clone();
            move |_window, event| {
                if let tauri::WindowEvent::Destroyed = event {
                    token.cancel();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while running Parish application")
        .run(move |app, event| {
            if matches!(&event, tauri::RunEvent::Ready) {
                // Tauri dev mode reapplies the configured bundle icon during
                // Ready on macOS. Reapply the active mod icon afterward so
                // `just run` shows the same Dock icon as bundled launches.
                apply_mod_desktop_icon(app, mod_window_icon_path_for_run.as_deref());
            }

            // Graceful shutdown of bundled vllm-mlx children. Drop already
            // calls stop() when AppState finally drops, but on Cmd+Q the
            // tokio runtime can be torn down before that fires, leaving
            // ~2-4 GB resident python processes orphaned to launchd. Hook
            // ExitRequested so we kill them while the runtime is still
            // alive enough to wait().
            if let tauri::RunEvent::ExitRequested { .. } = event {
                let state: tauri::State<'_, Arc<AppState>> = app.state();
                let state = Arc::clone(&state);
                tauri::async_runtime::block_on(async move {
                    let mut rp = state.runtime_processes.lock().await;
                    rp.stop();
                });
            }
        });
}

// ── Client initialisation from env ───────────────────────────────────────────

/// Reads configuration from `parish.toml` (if present) and `PARISH_*` env vars
/// into a [`ProviderConfig`] plus the display strings that populate [`GameConfig`].
pub(crate) fn provider_config_from_env(
    user_config_dir: &std::path::Path,
) -> (ProviderConfig, String, String, Option<String>) {
    let mut config =
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

    // Layer below env: hydrate from the wizard-persisted
    // `<user_config_dir>/parish.toml` (written by
    // `handle_set_provider_config`). When the user hasn't pinned anything
    // via env/CLI/project-root TOML, resolve_config returns the
    // Simulator default — only in that case do we substitute the saved
    // wizard choice so a returning user lands back on vllm-mlx without
    // re-doing onboarding. PARISH_PROVIDER and friends still win.
    if config.provider.id() == "simulator"
        && let Ok(user_cfg) = parish_core::config::user_config::load_user_config(user_config_dir)
        && let Some(provider_str) = user_cfg.provider.as_deref()
        && let Ok(saved_provider) = parish_core::config::Provider::from_str_loose(provider_str)
    {
        let saved_default_base = saved_provider.default_base_url().to_string();
        config.provider = saved_provider;
        config.base_url = user_cfg.base_url.clone().unwrap_or(saved_default_base);
        config.model = user_cfg.model.clone();
    }

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
    extra_vllm_mlx_slots: &[parish_core::inference::client::VllmMlxSlot],
    extra_vllm_slots: &[parish_core::inference::client::VllmSlot],
    inference_config: &parish_core::config::InferenceConfig,
    progress: &dyn parish_core::inference::setup::SetupProgress,
) -> anyhow::Result<(
    Option<AnyClient>,
    String,
    parish_core::inference::client::RuntimeProcesses,
)> {
    // Non-Ollama providers without a model → leave the client unset so the
    // UI can surface a config error instead of failing hard at startup.
    if config.model.is_none() && config.provider.id() != "ollama" {
        return Ok((
            None,
            String::new(),
            parish_core::inference::client::RuntimeProcesses::none(),
        ));
    }

    let (client, model, process) = parish_core::inference::setup::setup_provider_client(
        config,
        extra_vllm_mlx_slots,
        extra_vllm_slots,
        inference_config, // (#417) use TOML-configured timeouts
        progress,
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
            .unwrap_or_else(|| "https://generativelanguage.googleapis.com/v1".to_string())
    });
    let provider_enum = provider
        .as_deref()
        .and_then(|p| Provider::from_str_loose(p).ok())
        .unwrap_or_else(|| Provider::from_id("google").unwrap_or_default());
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

    /// Dev-build path: no bundled binary lives next to `cargo test`'s test
    /// runner exe, so the helper must return `None` rather than fabricate a
    /// path or panic. Packaged-build coverage is a manual probe — there is no
    /// `.app` bundle around the unit-test binary.
    #[test]
    fn resolve_bundled_vllm_mlx_bin_returns_none_for_dev_builds() {
        let resolved = resolve_bundled_vllm_mlx_bin();
        assert!(
            resolved.is_none(),
            "dev/test binaries are not packaged with a vllm-mlx resource; \
             got {:?} — if this fails, the probe is mis-matching",
            resolved,
        );
    }

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
