//! Shared application state and event bus for the web server.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Mutex;
// `tokio::sync::Mutex` used for `active_ws` so the guard can be held across
// await points without blocking Tokio workers.
use tokio::task::JoinHandle;

use parish_core::config::InferenceConfig;
use parish_core::debug_snapshot::DebugEvent;
use parish_core::game_mod::PronunciationEntry;
use parish_core::inference::{AnyClient, InferenceClient, InferenceLog, InferenceQueue};
use parish_core::ipc::ThemePalette;
use parish_core::npc::manager::NpcManager;
use parish_core::session_store::SessionStore;
use parish_core::world::WorldState;
use parish_core::world::events::GameEvent;
use parish_core::world::transport::TransportConfig;

// Re-export event-bus types: the concrete impl used for the AppState field,
// the trait (for emit_named calls at construction/test time), the wire type,
// and Topic (for test code).
pub use parish_core::event_bus::{
    BroadcastEventBus, EventBus as EventBusTrait, ServerEvent, Topic,
};

/// Maximum number of debug/game events retained in the server's ring buffer.
pub const DEBUG_EVENT_CAPACITY: usize = 100;

// ── Shared state types (moved to parish-core::ipc::state as part of #696) ───

/// Re-export from `parish_core` so all existing `crate::state::*` call sites
/// continue to compile without modification.
pub use parish_core::ipc::{ConversationRuntimeState, SaveState, UiConfigSnapshot};

/// Shared mutable game state for the web server.
///
/// Mirrors the Tauri `AppState` but uses a [`BroadcastEventBus`] for push
/// events instead of a Tauri `AppHandle`.
///
/// # Lock ordering invariant (#483)
///
/// `AppState` holds many independent `Mutex` fields. Any path that acquires
/// more than one at a time **must** follow the canonical order below. A
/// future refactor that takes these in the opposite order would deadlock
/// with any existing path that takes them in the documented order. The
/// ordering is derived from the paths actually observed in the codebase
/// today (`handle_system_command`, `handle_npc_conversation`,
/// `run_idle_banter`, `rebuild_inference`, `get_debug_snapshot`, and the
/// background tick tasks in `session.rs`).
///
/// ```text
/// world
///   → npc_manager
///     → inference_queue
///       → conversation
///         → config
///           → client
///             → cloud_client
///               → inference_client
///                 → debug_events
///                 → game_events
///                   → inference_log
///                     → editor_sessions
///                       → active_ws
///                         → save_path
///                           → current_branch_id
///                             → current_branch_name
///                               → worker_handle
///                                 → save_lock
///                                   → save_db
/// ```
///
/// Pair-by-pair rationale — every pair above is attested by at least
/// one current call site:
///
/// - `world → npc_manager` — every handler that touches both
///   (`handle_npc_conversation`, `run_idle_banter`, `get_debug_snapshot`,
///   the world-tick task).
/// - `npc_manager → inference_queue → config` — `handle_npc_conversation`
///   and `run_idle_banter` (`routes.rs`).
/// - `conversation → config` — `tick_inactivity` (`routes.rs`), so
///   `conversation` slots between `inference_queue` and `config`.
/// - `config → client` — `handle_game_input` (`routes.rs`).
/// - `config → debug_events → game_events → inference_log` —
///   `get_debug_snapshot` (`routes.rs`). `inference_log` is itself an
///   `Arc<Mutex<BoundedInferenceLog>>` (see
///   `parish-inference/src/lib.rs`), so it is a real coordination point,
///   not a lock-free buffer.
///
/// The remaining non-`Mutex` fields (`event_bus`, `transport`,
/// `ui_config`, `theme_palette`, `saves_dir`, `data_dir`, `game_mod`,
/// `pronunciations`, `flags_path`) are set once at startup and are not
/// coordination points.
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
    /// Stable identifier for this session — the same UUID that appears in the
    /// `parish_sid` cookie.  Stored here so background tasks (tick loop, etc.)
    /// can emit per-session tracing events without holding the middleware
    /// extensions (#621).
    pub session_id: String,
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
    /// Trait-erased inference client stack (caching + metrics decorator, #617).
    ///
    /// All inference call sites that use the new `InferenceClient` trait go
    /// through this `Arc`.  It is constructed by
    /// `parish_inference::build_inference_client_stack` and wraps `client` or
    /// `cloud_client` with an optional LRU cache and a metrics layer.
    ///
    /// `None` when no provider is configured (same lifecycle as `client`).
    ///
    /// Behind a `Mutex` so that `rebuild_inference` can swap it atomically
    /// when the provider/key changes at runtime — same lifecycle as `client`.
    /// Lock ordering: acquire after `cloud_client`, before `debug_events`.
    pub inference_client: Mutex<Option<Arc<dyn InferenceClient>>>,
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
    pub event_bus: BroadcastEventBus,
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
    /// Per-account editor sessions — keyed by `(account_id, mod_id)`.
    ///
    /// Keyed by `account_id` (stable UUID) so that multiple browser tabs from
    /// the same authenticated user share one editor session rather than creating
    /// per-cookie duplicates (#618).  The `String` component is the mod path /
    /// id, kept for future multi-mod support (currently always `""` until a mod
    /// is opened, at which point the session is re-keyed).
    ///
    /// When the `account-id-keying` feature flag is disabled the key is
    /// `(Uuid::nil(), email)` for backward compatibility.
    ///
    /// Uses a `tokio::sync::Mutex` so handlers can hold the guard across
    /// `.await` points without blocking Tokio workers.
    pub editor_sessions: tokio::sync::Mutex<
        std::collections::HashMap<(uuid::Uuid, String), parish_core::ipc::editor::EditorSession>,
    >,
    /// Set of `account_id`s that currently have an active WebSocket connection.
    ///
    /// Enforces single-WS-per-account (#334/#618): a second upgrade from the
    /// same account is rejected with 409 Conflict until the first socket closes.
    /// Uses a `tokio::sync::Mutex` so it can be held across await points.
    pub active_ws: tokio::sync::Mutex<HashSet<uuid::Uuid>>,
    /// Advisory file lock for the currently active save file.
    pub save_lock: Mutex<Option<parish_core::persistence::SaveFileLock>>,
    /// TOML-configured inference timeouts loaded from `parish.toml` at session
    /// creation.  Stored here so runtime rebuilds (e.g. after `/provider`) use
    /// the operator-configured values instead of the compiled-in defaults. (#417)
    pub inference_config: InferenceConfig,
    /// Cached async database handle for the currently active save file.
    ///
    /// Opened lazily by the autosave tick and reused across ticks to avoid
    /// re-running `migrate()` and re-opening the WAL file on every 60-second
    /// interval (#230).  Stored as `(path, db)` so the tick can detect when
    /// `save_path` has changed and reopen accordingly.
    ///
    /// Lock ordering: acquired after `save_lock`.
    pub save_db:
        tokio::sync::Mutex<Option<(std::path::PathBuf, parish_core::persistence::AsyncDatabase)>>,
    /// Trait-erased per-session persistence.
    ///
    /// Route handlers and the autosave tick should prefer this over direct
    /// `Database` / `AsyncDatabase` calls so that future remote or managed-auth
    /// backends can be swapped in without touching handler code (#614).
    ///
    /// Not part of the lock-ordering chain: `Arc<dyn SessionStore>` is
    /// never held across lock acquisition of any `Mutex` field.
    pub session_store: Arc<dyn SessionStore>,
    /// Setup status snapshot (always `done: true` for the web server since
    /// there is no Ollama bootstrap process).  Present for IPC wiring parity
    /// with the Tauri backend (#732).
    pub setup_status: std::sync::Mutex<SetupStatusSnapshot>,
    /// Language settings derived from the active mod manifest.
    ///
    /// Resolved once at startup and injected into all dialogue prompt builders
    /// to enforce locale-correct spelling and code-switching behaviour.
    pub language_settings: parish_core::npc::LanguageSettings,
}

/// Server-side counterpart of the Tauri `SetupStatusSnapshot`.
///
/// Always initialised as `done: true` because the web server has no Ollama
/// bootstrap flow — this exists purely for IPC command parity (#732).
#[derive(serde::Serialize, Clone)]
pub struct SetupStatusSnapshot {
    pub current_message: String,
    pub messages: Vec<String>,
    pub completed: u64,
    pub total: u64,
    pub done: bool,
    pub success: Option<bool>,
    pub error: String,
}

impl Default for SetupStatusSnapshot {
    fn default() -> Self {
        Self {
            current_message: String::new(),
            messages: vec![],
            completed: 0,
            total: 0,
            done: true,
            success: Some(true),
            error: String::new(),
        }
    }
}

// GameConfig is now shared across all backends via parish-core.
pub use parish_core::ipc::GameConfig;

/// Creates the shared [`AppState`] from game data.
// AppState is a flat bundle of all server-wide singletons; a builder pattern
// would add complexity without benefit, so the many-argument constructor is intentional.
#[allow(clippy::too_many_arguments)]
pub fn build_app_state(
    session_id: String,
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
    inference_config: InferenceConfig,
    session_store: Arc<dyn SessionStore>,
) -> Arc<AppState> {
    // Extract pronunciations from game mod before moving it.
    let pronunciations = game_mod
        .as_ref()
        .map(|gm| gm.pronunciations.clone())
        .unwrap_or_default();

    // Extract language settings from game mod
    let language_settings = game_mod
        .as_ref()
        .map(|gm| {
            parish_core::npc::LanguageSettings::new(
                gm.player_language().to_string(),
                gm.native_language().map(str::to_string),
            )
        })
        .unwrap_or_else(parish_core::npc::LanguageSettings::english_only);

    // Build the trait-erased inference client stack (#617).
    // Feature flags are not yet loaded at this point (flags_path not read),
    // so we default both inference-client-trait and inference-response-cache
    // to on (the per-CLAUDE.md §6 default-on convention).  Routes/handlers
    // that need the flag-gated behaviour should check flags at call time.
    let inference_client = client.as_ref().map(|c| {
        let cache_capacity = parish_core::inference::cache_capacity_from_env();
        parish_core::inference::build_inference_client_stack(c.clone(), true, cache_capacity)
    });

    Arc::new(AppState {
        session_id,
        world: Mutex::new(world),
        npc_manager: Mutex::new(npc_manager),
        inference_queue: Mutex::new(None),
        inference_log: parish_core::inference::new_inference_log(),
        client: Mutex::new(client),
        cloud_client: Mutex::new(cloud_client),
        inference_client: Mutex::new(inference_client),
        config: Mutex::new(config),
        conversation: Mutex::new(ConversationRuntimeState::new()),
        debug_events: Mutex::new(std::collections::VecDeque::with_capacity(
            DEBUG_EVENT_CAPACITY,
        )),
        game_events: Mutex::new(std::collections::VecDeque::with_capacity(
            DEBUG_EVENT_CAPACITY,
        )),
        event_bus: BroadcastEventBus::new(256),
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
        active_ws: tokio::sync::Mutex::new(HashSet::<uuid::Uuid>::new()),
        save_lock: Mutex::new(None),
        inference_config,
        save_db: tokio::sync::Mutex::new(None),
        session_store,
        setup_status: std::sync::Mutex::new(SetupStatusSnapshot::default()),
        language_settings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use parish_core::event_bus::{EventBus as EventBusTrait, Topic};

    #[test]
    fn event_bus_emit_named_and_subscribe() {
        let bus = BroadcastEventBus::new(16);
        let mut stream = bus.subscribe(&[]);
        bus.emit_named(
            Topic::TextLog,
            "test-event",
            &serde_json::json!({"key": "value"}),
        );
        let event = stream.try_recv().unwrap();
        assert_eq!(event.event, "test-event");
        assert_eq!(event.payload["key"], "value");
    }

    #[test]
    fn event_bus_no_subscribers_does_not_panic() {
        let bus = BroadcastEventBus::new(16);
        // No subscribers — should not panic
        bus.emit(
            Topic::TextLog,
            ServerEvent {
                event: "orphan".to_string(),
                payload: serde_json::Value::Null,
            },
        );
    }
}
