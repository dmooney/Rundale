//! Shared application state and event bus for the web server.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::{Mutex, broadcast};
// `tokio::sync::Mutex` used for `active_ws` so the guard can be held across
// await points without blocking Tokio workers.
use tokio::task::JoinHandle;

use parish_core::debug_snapshot::DebugEvent;
use parish_core::game_mod::PronunciationEntry;
use parish_core::inference::{AnyClient, InferenceLog, InferenceQueue};
use parish_core::ipc::ConversationLine;
use parish_core::ipc::ThemePalette;
use parish_core::npc::manager::NpcManager;
use parish_core::world::events::GameEvent;
use parish_core::world::transport::TransportConfig;
use parish_core::world::{LocationId, WorldState};

/// Maximum number of debug/game events retained in the server's ring buffer.
pub const DEBUG_EVENT_CAPACITY: usize = 100;

/// UI configuration snapshot returned by the `/api/ui-config` endpoint.
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
}

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

/// Runtime conversation/session state used for multi-NPC continuity and idle timers.
pub struct ConversationRuntimeState {
    /// Player location associated with the current transcript.
    pub location: Option<LocationId>,
    /// Recent dialogue at the current location.
    pub transcript: std::collections::VecDeque<ConversationLine>,
    /// Last wall-clock moment when the player submitted input.
    pub last_player_activity: Instant,
    /// Last wall-clock moment when anyone said something in the local conversation.
    pub last_spoken_at: Instant,
    /// Whether a player- or idle-triggered NPC exchange is currently running.
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

/// Shared mutable game state for the web server.
///
/// Mirrors the Tauri `AppState` but uses an [`EventBus`] for push events
/// instead of a Tauri `AppHandle`.
///
/// # Lock ordering invariant (#483)
///
/// `AppState` holds many independent `Mutex` fields. Any path that acquires
/// more than one at a time **must** follow the canonical order below. A
/// future refactor that holds, say, `config` while acquiring `world` would
/// deadlock with any existing path that holds `world` while acquiring
/// `config`. The ordering is derived from the paths observed in the
/// codebase today (`handle_system_command`, `rebuild_inference`,
/// `get_debug_snapshot`, background ticks).
///
/// ```text
/// world
///   → npc_manager
///     → config
///       → client
///         → cloud_client
///           → inference_queue
///             → conversation
///               → debug_events
///                 → game_events
///                   → editor_sessions
///                     → active_ws
///                       → save_path
///                         → current_branch_id
///                           → current_branch_name
///                             → worker_handle
///                               → save_lock
/// ```
///
/// `inference_log` is a lock-free ring buffer and safe to access at any
/// level of the stack; the other non-Mutex fields (`event_bus`,
/// `transport`, `ui_config`, `theme_palette`, `saves_dir`, `data_dir`,
/// `game_mod`, `pronunciations`, `flags_path`) are set once at startup
/// and are not coordination points.
///
/// **Release locks promptly.** The preferred idiom is to scope each guard
/// to the smallest possible block and drop it before acquiring the next,
/// both to minimise lock-held time and to make deadlocks (if any) easier
/// to spot in a diff. When a nested acquire is truly required — for
/// example copying an NPC summary into a world-side buffer — acquire the
/// locks in the order above and drop them in reverse.
///
/// **Don't hold these locks across `.await` on network I/O.** See #464
/// and editor_save for the pattern: clone what you need out of the lock,
/// release, do the blocking/async work, then re-acquire briefly to
/// install the result. Holding `config` or `client` across an HTTP
/// refresh, or `world` across a save-to-disk, will serialise every
/// concurrent session behind that one path.
pub struct AppState {
    /// The game world (clock, player position, graph, weather).
    pub world: Mutex<WorldState>,
    /// NPC manager (all NPCs, tier assignment, schedule ticking).
    pub npc_manager: Mutex<NpcManager>,
    /// Inference request queue (None if no provider configured).
    pub inference_queue: Mutex<Option<InferenceQueue>>,
    /// Shared ring buffer of recent inference calls (for the debug panel).
    pub inference_log: InferenceLog,
    /// Local LLM client (None if no provider is configured).
    pub client: Mutex<Option<AnyClient>>,
    /// Cloud LLM client for dialogue (None if not configured).
    pub cloud_client: Mutex<Option<AnyClient>>,
    /// Mutable runtime configuration.
    pub config: Mutex<GameConfig>,
    /// Local conversation transcript and inactivity tracking.
    pub conversation: Mutex<ConversationRuntimeState>,
    /// Rolling ring buffer of debug events (schedule ticks, tier transitions,
    /// inference errors) surfaced to the debug panel.
    pub debug_events: Mutex<std::collections::VecDeque<DebugEvent>>,
    /// Rolling ring buffer of `GameEvent`s captured from the world event bus.
    pub game_events: Mutex<std::collections::VecDeque<GameEvent>>,
    /// Broadcast channel for pushing events to WebSocket clients.
    pub event_bus: EventBus,
    /// Transport mode configuration from the loaded game mod.
    pub transport: TransportConfig,
    /// UI configuration from the loaded game mod.
    pub ui_config: UiConfigSnapshot,
    /// Fixed theme palette from the loaded game mod.
    pub theme_palette: ThemePalette,
    /// Directory where save files are stored.
    pub saves_dir: PathBuf,
    /// Directory containing game data files (world.json, npcs.json, etc.).
    pub data_dir: PathBuf,
    /// Path to the currently active save file.
    pub save_path: Mutex<Option<PathBuf>>,
    /// Current branch database id.
    pub current_branch_id: Mutex<Option<i64>>,
    /// Current branch name.
    pub current_branch_name: Mutex<Option<String>>,
    /// Loaded game mod data (for reaction templates, etc.).
    pub game_mod: Option<parish_core::game_mod::GameMod>,
    /// Name pronunciation entries from the game mod.
    pub pronunciations: Vec<PronunciationEntry>,
    /// Path to the feature flags persistence file.
    pub flags_path: PathBuf,
    /// Handle for the active inference worker task; used to abort it on rebuild
    /// or shutdown so orphaned workers (each holding an HTTP client and channel)
    /// don't accumulate.  See bugs #224 and #231.
    pub worker_handle: Mutex<Option<JoinHandle<()>>>,
    /// Per-user editor sessions — keyed by CF-Access email.
    ///
    /// Uses a `tokio::sync::Mutex` so handlers can hold the guard across
    /// `.await` points without blocking Tokio workers.
    pub editor_sessions: tokio::sync::Mutex<
        std::collections::HashMap<String, parish_core::ipc::editor::EditorSession>,
    >,
    /// Set of emails that currently have an active WebSocket connection.
    ///
    /// Enforces single-WS-per-email (#334): a second upgrade from the same
    /// email is rejected with 409 Conflict until the first socket closes.
    /// Uses a `tokio::sync::Mutex` so it can be held across await points.
    pub active_ws: tokio::sync::Mutex<HashSet<String>>,
    /// Advisory file lock for the currently active save file.
    pub save_lock: Mutex<Option<parish_core::persistence::SaveFileLock>>,
}

// GameConfig is now shared across all backends via parish-core.
pub use parish_core::ipc::GameConfig;

/// A JSON-serializable server event pushed to WebSocket clients.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ServerEvent {
    /// Event name (e.g. "stream-token", "text-log").
    pub event: String,
    /// JSON payload for this event.
    pub payload: serde_json::Value,
}

/// Broadcast channel for server-push events.
///
/// Events emitted here are forwarded to all connected WebSocket clients.
pub struct EventBus {
    tx: broadcast::Sender<ServerEvent>,
}

impl EventBus {
    /// Creates a new event bus with the given channel capacity.
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    /// Sends an event to all subscribers. Returns the number of receivers.
    pub fn send(&self, event: ServerEvent) -> usize {
        match self.tx.send(event) {
            Ok(count) => count,
            Err(_) => {
                tracing::warn!("EventBus: broadcast failed — no active subscribers");
                0
            }
        }
    }

    /// Emits a named event with a serializable payload.
    pub fn emit<T: serde::Serialize>(&self, event_name: &str, payload: &T) {
        match serde_json::to_value(payload) {
            Ok(value) => {
                self.send(ServerEvent {
                    event: event_name.to_string(),
                    payload: value,
                });
            }
            Err(e) => {
                tracing::warn!(event = %event_name, error = %e, "EventBus: failed to serialize event payload");
            }
        }
    }

    /// Creates a new receiver for this bus.
    pub fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        self.tx.subscribe()
    }
}

/// Creates the shared [`AppState`] from game data.
// AppState is a flat bundle of all server-wide singletons; a builder pattern
// would add complexity without benefit, so the many-argument constructor is intentional.
#[allow(clippy::too_many_arguments)]
pub fn build_app_state(
    world: WorldState,
    npc_manager: NpcManager,
    client: Option<AnyClient>,
    config: GameConfig,
    cloud_client: Option<AnyClient>,
    transport: TransportConfig,
    ui_config: UiConfigSnapshot,
    theme_palette: ThemePalette,
    saves_dir: PathBuf,
    data_dir: PathBuf,
    game_mod: Option<parish_core::game_mod::GameMod>,
    flags_path: PathBuf,
) -> Arc<AppState> {
    // Extract pronunciations from game mod before moving it.
    let pronunciations = game_mod
        .as_ref()
        .map(|gm| gm.pronunciations.clone())
        .unwrap_or_default();
    Arc::new(AppState {
        world: Mutex::new(world),
        npc_manager: Mutex::new(npc_manager),
        inference_queue: Mutex::new(None),
        inference_log: parish_core::inference::new_inference_log(),
        client: Mutex::new(client),
        cloud_client: Mutex::new(cloud_client),
        config: Mutex::new(config),
        conversation: Mutex::new(ConversationRuntimeState::new()),
        debug_events: Mutex::new(std::collections::VecDeque::with_capacity(
            DEBUG_EVENT_CAPACITY,
        )),
        game_events: Mutex::new(std::collections::VecDeque::with_capacity(
            DEBUG_EVENT_CAPACITY,
        )),
        event_bus: EventBus::new(256),
        transport,
        ui_config,
        theme_palette,
        saves_dir,
        data_dir,
        save_path: Mutex::new(None),
        current_branch_id: Mutex::new(None),
        current_branch_name: Mutex::new(None),
        game_mod,
        pronunciations,
        flags_path,
        worker_handle: Mutex::new(None),
        editor_sessions: tokio::sync::Mutex::new(std::collections::HashMap::new()),
        active_ws: tokio::sync::Mutex::new(HashSet::new()),
        save_lock: Mutex::new(None),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_bus_send_and_subscribe() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();
        bus.emit("test-event", &serde_json::json!({"key": "value"}));
        let event = rx.try_recv().unwrap();
        assert_eq!(event.event, "test-event");
        assert_eq!(event.payload["key"], "value");
    }

    #[test]
    fn event_bus_no_subscribers() {
        let bus = EventBus::new(16);
        // No subscribers — should not panic
        let count = bus.send(ServerEvent {
            event: "orphan".to_string(),
            payload: serde_json::Value::Null,
        });
        assert_eq!(count, 0);
    }
}
