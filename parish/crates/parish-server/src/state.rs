//! Shared application state and event bus for the web server.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::task::JoinHandle;

// All coordination fields use `MeteredMutex` — a `tokio::sync::Mutex` wrapper
// that records wait-time/contention counters (#1366 §2) — so guards can be
// held across await points without blocking Tokio workers.
use crate::lock_metrics::{LockMetricsSnapshot, MeteredMutex};

use parish_core::config::InferenceConfig;
use parish_core::debug_snapshot::DebugEvent;
use parish_core::game_mod::PronunciationEntry;
use parish_core::inference::{AnyClient, InferenceLog, InferenceQueue};
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

/// Canonical acquisition order for the `AppState` coordination locks (#483, Rule 11).
///
/// **Single source of truth.** Any path that acquires more than one
/// `AppState` lock at a time must take them in this order; taking two in the
/// opposite order risks a deadlock against an existing path. The
/// `lock_order_fitness` sensor in `tests/lock_order_fitness.rs` reads this
/// array and rejects any out-of-order adjacent acquisition pair in the server
/// handler/session/route bodies.
///
/// Entries are **group/field node names**, not the dissolved members: the
/// inference-client trio (`client` / `cloud_client` / `inference_queue`) now
/// lives behind the single `inference` node, and the save-identity trio
/// (`save_path` / `current_branch_id` / `current_branch_name`) behind the
/// single `save_identity` node. A reference to a dissolved member maps to its
/// group node for ordering purposes (see the sensor's `group_of`).
pub const LOCK_ORDER: &[&str] = &[
    "persistence_gate",
    "world",
    "npc_manager",
    "conversation",
    "config",
    "inference",
    "debug_events",
    "game_events",
    "inference_log",
    "editor_sessions",
    "active_ws",
    "save_identity",
    "worker_handle",
    "save_lock",
    "save_db",
];

// ── Shared state types (moved to parish-core::ipc::state as part of #696) ───

/// Re-export from `parish_core` so all existing `crate::state::*` call sites
/// continue to compile without modification.
pub use parish_core::ipc::{ConversationRuntimeState, SaveState, UiConfigSnapshot};

/// Shared mutable game state for the web server.
///
/// Mirrors the Tauri `AppState` but uses a [`BroadcastEventBus`] for push
/// events instead of a Tauri `AppHandle`.
///
/// # Lock ordering invariant (#483, Rule 11)
///
/// `AppState` holds many independent `Mutex` fields. Any path that acquires
/// more than one at a time **must** follow the canonical order encoded in
/// the [`LOCK_ORDER`] const (the single source of truth — see its docs for
/// the full list). A future refactor that takes two in the opposite order
/// would deadlock with any existing path that takes them in the documented
/// order; the `lock_order_fitness` sensor enforces this mechanically.
///
/// As part of #1366 §2 the inference-client trio (`client` / `cloud_client`
/// / `inference_queue`) was grouped behind the single [`InferenceClients`]
/// node `inference`, and the save-identity trio (`save_path` /
/// `current_branch_id` / `current_branch_name`) behind the single
/// [`SaveIdentity`] node `save_identity`. This is a change of lock
/// _granularity_, not of any *attested* pair's order. In the pre-grouping
/// chain the three inference members straddled `config`: `inference_queue`
/// sat early, while `client` / `cloud_client` sat just after `config`.
/// Collapsing them into one node requires a single position, and the
/// `inference` node is placed at the `client` / `cloud_client` slot (just
/// after `config`) because that is the only position any *held* inference
/// guard is ever acquired — `inference_queue` is only ever read as a released
/// temporary (`.is_some()`), never held across another lock, so vacating its
/// former early slot inverts no real acquisition. Every held pair observed in
/// the code (e.g. `config` → `inference.client` in the tier-2 tick and
/// reactions paths) is in canonical order under this placement.
///
/// The ordering is derived from the paths actually observed in the codebase
/// (`handle_system_command`, `handle_npc_conversation`, `run_idle_banter`,
/// `rebuild_inference`, `get_debug_snapshot`, and the background tick tasks
/// in `session/ticks.rs`). Every adjacent pair in [`LOCK_ORDER`] is attested
/// by at least one current call site; `inference_log` is itself an
/// `Arc<Mutex<BoundedInferenceLog>>` (see `parish-inference/src/lib.rs`), so
/// it is a real coordination point, not a lock-free buffer.
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
/// Inference-client coordination group (#1366 §2).
///
/// Bundles the three inference-client locks (`client`, `cloud_client`,
/// `inference_queue`) that travel together in the lock chain into a single
/// `inference` node. The members stay individually `Mutex`-wrapped so the
/// server can hand out per-field `&Mutex<…>` borrows to the shared
/// `parish_core::game_loop` API (`GameLoopContext`), which is constructed by
/// all three entry points and must keep its per-field reference shape (C6 /
/// CS-6). Grouping reduces this trio to one named node in [`LOCK_ORDER`]
/// without changing the relative lock order or the shared struct signatures.
///
/// `config` is **deliberately not** folded in (CS-4): it is acquired far more
/// often than any inference client and on paths with no client at all, so
/// merging it would widen the critical section rather than shrink it.
pub struct InferenceClients {
    /// Local LLM client (None if no provider is configured).
    pub client: MeteredMutex<Option<AnyClient>>,
    /// Cloud LLM client for dialogue (None if not configured).
    pub cloud_client: MeteredMutex<Option<AnyClient>>,
    /// Inference request queue (None if no provider configured).
    pub inference_queue: MeteredMutex<Option<InferenceQueue>>,
}

/// Save-identity coordination group (#1366 §2).
///
/// Bundles the three save-identity scalars (`save_path`, `current_branch_id`,
/// `current_branch_name`) that are acquired together in the save / fork / load
/// hotspots into a single `save_identity` node. As with [`InferenceClients`]
/// the members stay individually `Mutex`-wrapped so the server can pass
/// per-field `&Mutex<…>` borrows to `parish_core::game_loop::NewGameParams`
/// without changing the shared struct (C6 / CS-6).
///
/// `save_lock` and `save_db` are **not** in this group (CS-5): they have
/// distinct lifetimes, sit at the tail of the chain, and are acquired
/// independently of the identity triple (e.g. the autosave tick takes
/// `save_db` without re-taking the identity).
pub struct SaveIdentity {
    /// Path to the currently active save file.
    pub save_path: MeteredMutex<Option<PathBuf>>,
    /// Current branch database id.
    pub current_branch_id: MeteredMutex<Option<i64>>,
    /// Current branch name.
    pub current_branch_name: MeteredMutex<Option<String>>,
}

impl SaveIdentity {
    /// Captures the active save file and branch under one lock-order-consistent
    /// critical section so a concurrent file switch cannot produce a torn pair.
    pub async fn task_journal_target(
        &self,
        session_id: &str,
    ) -> Option<parish_core::session_store::TaskJournalTarget> {
        let save_path = self.save_path.lock().await;
        let branch_id = self.current_branch_id.lock().await;
        Some(parish_core::session_store::TaskJournalTarget {
            session_id: session_id.to_string(),
            save_path: save_path.as_ref()?.clone(),
            branch_id: (*branch_id)?,
        })
    }

    /// Replaces the active save identity atomically in the documented
    /// `save_path → branch id → branch name` lock order.
    pub async fn replace(&self, path: PathBuf, branch_id: i64, branch_name: String) {
        let mut save_path = self.save_path.lock().await;
        let mut current_branch_id = self.current_branch_id.lock().await;
        let mut current_branch_name = self.current_branch_name.lock().await;
        *save_path = Some(path);
        *current_branch_id = Some(branch_id);
        *current_branch_name = Some(branch_name);
    }
}

pub struct AppState {
    /// Stable identifier for this session — the same UUID that appears in the
    /// `parish_sid` cookie.  Stored here so background tasks (tick loop, etc.)
    /// can emit per-session tracing events without holding the middleware
    /// extensions (#621).
    pub session_id: String,
    /// Outermost per-session persistence and lifecycle barrier.
    ///
    /// Held from before task pre-state/identity capture through journal
    /// commit (or rollback), and from before snapshot capture through save,
    /// load, fork, or new-game identity commit. It must always be acquired
    /// before every other `AppState` coordination lock.
    pub persistence_gate: MeteredMutex<()>,
    /// The game world (clock, player position, graph, weather).
    pub world: MeteredMutex<WorldState>,
    /// NPC manager (all NPCs, tier assignment, schedule ticking).
    pub npc_manager: MeteredMutex<NpcManager>,
    /// Inference-client coordination group — `client` / `cloud_client` /
    /// `inference_queue` behind the single `inference` node (#1366 §2).
    pub inference: InferenceClients,
    /// Shared ring buffer of recent inference calls (for the debug panel).
    pub inference_log: InferenceLog,
    /// Mutable runtime configuration.
    pub config: MeteredMutex<GameConfig>,
    /// Local conversation transcript and inactivity tracking.
    pub conversation: MeteredMutex<ConversationRuntimeState>,
    /// Rolling ring buffer of debug events (schedule ticks, tier transitions,
    /// inference errors) surfaced to the debug panel.
    pub debug_events: MeteredMutex<std::collections::VecDeque<DebugEvent>>,
    /// Rolling ring buffer of `GameEvent`s captured from the world event bus.
    pub game_events: MeteredMutex<std::collections::VecDeque<GameEvent>>,
    /// Monotonic lifetime count of events pushed into `game_events`.
    ///
    /// `GET /api/turn?since=N` uses this counter rather than the bounded ring
    /// length so its cursor keeps advancing after the ring wraps.
    pub total_game_events: std::sync::atomic::AtomicUsize,
    /// Broadcast channel for pushing events to WebSocket clients.
    pub event_bus: BroadcastEventBus,
    /// Transport mode configuration from the loaded game mod.
    pub transport: TransportConfig,
    /// UI configuration from the loaded game mod.
    pub ui_config: UiConfigSnapshot,
    /// Fixed theme palette from the loaded game mod.
    pub theme_palette: ThemePalette,
    /// Time-of-day palette keyframes from the loaded game mod. Empty when the
    /// mod ships only a static palette (or no mod is loaded).
    pub theme_keyframes: Vec<parish_palette::Keyframe>,
    /// Static palette in `RawPalette` form, used by `get_theme` when the mod
    /// declares no keyframes. `None` when no mod is loaded (engine falls back
    /// to `neutral_grey_palette`).
    pub static_raw_palette: Option<parish_palette::RawPalette>,
    /// Atmospheric flavour shown when NPC inference fails. Empty when the
    /// mod provides none — engine falls back to a single ellipsis.
    pub inference_failure_messages: Vec<String>,
    /// Atmospheric flavour shown when the player addresses no-one. Empty
    /// when the mod provides none — engine falls back to a blank line.
    pub idle_messages: Vec<String>,
    /// Directory where save files are stored.
    pub saves_dir: PathBuf,
    /// Directory containing game data files (world.json, npcs.json, etc.).
    pub data_dir: PathBuf,
    /// Save-identity coordination group — `save_path` / `current_branch_id` /
    /// `current_branch_name` behind the single `save_identity` node (#1366 §2).
    pub save_identity: SaveIdentity,
    /// Loaded game mod data (for reaction templates, etc.).
    pub game_mod: Option<parish_core::game_mod::GameMod>,
    /// Name pronunciation entries from the game mod.
    pub pronunciations: Vec<PronunciationEntry>,
    /// Path to the feature flags persistence file.
    pub flags_path: PathBuf,
    /// Handle for the active inference worker task; used to abort it on rebuild
    /// or shutdown so orphaned workers (each holding an HTTP client and channel)
    /// don't accumulate.  See bugs #224 and #231.
    pub worker_handle: MeteredMutex<Option<JoinHandle<()>>>,
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
    pub editor_sessions: MeteredMutex<
        std::collections::HashMap<(uuid::Uuid, String), parish_core::ipc::editor::EditorSession>,
    >,
    /// Set of `account_id`s that currently have an active WebSocket connection.
    ///
    /// Enforces single-WS-per-account (#334/#618): a second upgrade from the
    /// same account is rejected with 409 Conflict until the first socket closes.
    /// Uses a `tokio::sync::Mutex` so it can be held across await points.
    pub active_ws: MeteredMutex<HashSet<uuid::Uuid>>,
    /// Advisory file lock for the currently active save file.
    pub save_lock: MeteredMutex<Option<parish_core::persistence::SaveFileLock>>,
    /// TOML-configured inference timeouts loaded from `parish.toml` at session
    /// creation.  Stored here so runtime rebuilds (e.g. after `/provider`) use
    /// the operator-configured values instead of the compiled-in defaults. (#417)
    pub inference_config: InferenceConfig,
    /// Persistent on-disk inference call log.  Cheap to clone (`Arc` internals);
    /// shared with the inference worker so every call is captured. Toggled by
    /// the `/inference-log` slash command at runtime.
    pub inference_file_log: parish_core::inference::file_log::InferenceFileLog,
    /// Persistent on-disk chat transcript log. Same enable flag as
    /// `inference_file_log`; correlation between the two is via the shared
    /// `parish.request_id` field.
    pub chat_transcript_log: parish_core::chat_transcript::ChatTranscriptLog,
    /// Cached async database handle for the currently active save file.
    ///
    /// Opened lazily by the autosave tick and reused across ticks to avoid
    /// re-running `migrate()` and re-opening the WAL file on every 60-second
    /// interval (#230).  Stored as `(path, db)` so the tick can detect when
    /// `save_path` has changed and reopen accordingly.
    ///
    /// Lock ordering: acquired after `save_lock`.
    pub save_db:
        MeteredMutex<Option<(std::path::PathBuf, parish_core::persistence::AsyncDatabase)>>,
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
    /// Serialises concurrent `/api/command` calls on this session.
    ///
    /// A second call while one is in flight gets HTTP 409 rather than racing
    /// on the event-bus drain.  `try_lock` is used so the handler never blocks.
    pub command_lock: MeteredMutex<()>,
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

impl AppState {
    /// Returns the current contention counters for every metered `AppState`
    /// lock (#1366 §2 — the evidence base for any future lock splitting).
    pub fn lock_metrics(&self) -> Vec<LockMetricsSnapshot> {
        vec![
            self.persistence_gate.metrics(),
            self.world.metrics(),
            self.npc_manager.metrics(),
            self.conversation.metrics(),
            self.config.metrics(),
            self.inference.client.metrics(),
            self.inference.cloud_client.metrics(),
            self.inference.inference_queue.metrics(),
            self.debug_events.metrics(),
            self.game_events.metrics(),
            self.editor_sessions.metrics(),
            self.active_ws.metrics(),
            self.save_identity.save_path.metrics(),
            self.save_identity.current_branch_id.metrics(),
            self.save_identity.current_branch_name.metrics(),
            self.worker_handle.metrics(),
            self.save_lock.metrics(),
            self.save_db.metrics(),
            self.command_lock.metrics(),
        ]
    }

    /// Returns the canonical absolute path of the project's `mods/` directory.
    ///
    /// Resolves from startup state per Rule 9 — never calls `current_dir()`.
    /// Resolution order:
    /// 1. Parent of the loaded `game_mod.mod_dir` (present on all real runs).
    /// 2. Parent of the path returned by `find_default_mod()` (dev fallback).
    /// 3. Relative `"mods"` with a warning (packaged builds should never reach here).
    pub fn mods_root(&self) -> PathBuf {
        if let Some(ref gm) = self.game_mod
            && let Some(parent) = gm.mod_dir.parent()
        {
            return parent.to_path_buf();
        }
        parish_core::game_mod::find_default_mod()
            .and_then(|p| p.parent().map(|pp| pp.to_path_buf()))
            .unwrap_or_else(|| {
                let fallback = PathBuf::from("mods");
                tracing::warn!(
                    path = %fallback.display(),
                    "Could not find mods directory from game mod or workspace — falling back to \
                     relative path. The editor may list no mods on packaged builds."
                );
                fallback
            })
    }
}

/// All inputs required to construct the shared [`AppState`].
///
/// `AppState` is a flat bundle of every server-wide singleton; grouping its
/// constructor inputs into one struct keeps the call sites named and ordering-
/// independent so [`build_app_state`] no longer needs
/// `#[allow(clippy::too_many_arguments)]` (TD-037).
pub struct AppStateParts {
    pub session_id: String,
    pub world: WorldState,
    pub npc_manager: NpcManager,
    pub client: Option<AnyClient>,
    pub config: GameConfig,
    pub cloud_client: Option<AnyClient>,
    pub transport: TransportConfig,
    pub ui_config: UiConfigSnapshot,
    pub theme_palette: ThemePalette,
    pub saves_dir: PathBuf,
    pub data_dir: PathBuf,
    pub game_mod: Option<parish_core::game_mod::GameMod>,
    pub flags_path: PathBuf,
    pub inference_config: InferenceConfig,
    pub session_store: Arc<dyn SessionStore>,
    pub inference_file_log: parish_core::inference::file_log::InferenceFileLog,
    pub chat_transcript_log: parish_core::chat_transcript::ChatTranscriptLog,
}

/// Creates the shared [`AppState`] from game data.
pub fn build_app_state(parts: AppStateParts) -> Arc<AppState> {
    let AppStateParts {
        session_id,
        world,
        npc_manager,
        client,
        config,
        cloud_client,
        transport,
        ui_config,
        theme_palette,
        saves_dir,
        data_dir,
        game_mod,
        flags_path,
        inference_config,
        session_store,
        inference_file_log,
        chat_transcript_log,
    } = parts;

    // Extract pronunciations from game mod before moving it.
    let pronunciations = game_mod
        .as_ref()
        .map(|gm| gm.pronunciations.clone())
        .unwrap_or_default();

    // Extract palette runtime state (keyframes + static) from the loaded mod.
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

    Arc::new(AppState {
        session_id,
        persistence_gate: MeteredMutex::new("persistence_gate", ()),
        world: MeteredMutex::new("world", world),
        npc_manager: MeteredMutex::new("npc_manager", npc_manager),
        inference: InferenceClients {
            client: MeteredMutex::new("inference.client", client),
            cloud_client: MeteredMutex::new("inference.cloud_client", cloud_client),
            inference_queue: MeteredMutex::new("inference.inference_queue", None),
        },
        inference_log: parish_core::inference::new_inference_log(),
        config: MeteredMutex::new("config", config),
        conversation: MeteredMutex::new("conversation", ConversationRuntimeState::new()),
        debug_events: MeteredMutex::new(
            "debug_events",
            std::collections::VecDeque::with_capacity(DEBUG_EVENT_CAPACITY),
        ),
        game_events: MeteredMutex::new(
            "game_events",
            std::collections::VecDeque::with_capacity(DEBUG_EVENT_CAPACITY),
        ),
        total_game_events: std::sync::atomic::AtomicUsize::new(0),
        event_bus: BroadcastEventBus::new(256),
        transport,
        ui_config,
        theme_palette,
        theme_keyframes,
        static_raw_palette,
        inference_failure_messages,
        idle_messages,
        saves_dir,
        data_dir,
        save_identity: SaveIdentity {
            save_path: MeteredMutex::new("save_identity.save_path", None),
            current_branch_id: MeteredMutex::new("save_identity.current_branch_id", None),
            current_branch_name: MeteredMutex::new("save_identity.current_branch_name", None),
        },
        game_mod,
        pronunciations,
        flags_path,
        worker_handle: MeteredMutex::new("worker_handle", None),
        editor_sessions: MeteredMutex::new("editor_sessions", std::collections::HashMap::new()),
        active_ws: MeteredMutex::new("active_ws", HashSet::<uuid::Uuid>::new()),
        save_lock: MeteredMutex::new("save_lock", None),
        inference_config,
        save_db: MeteredMutex::new("save_db", None),
        session_store,
        setup_status: std::sync::Mutex::new(SetupStatusSnapshot::default()),
        language_settings,
        command_lock: MeteredMutex::new("command_lock", ()),
        inference_file_log,
        chat_transcript_log,
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

    // ── AppStateParts builder (TD-037) ───────────────────────────────────────

    fn test_game_config() -> GameConfig {
        GameConfig {
            inference_routes_v2: Default::default(),
            inference_subrole_routes_v2: Default::default(),
            inference_configuration_epoch: 0,
            provider_name: String::new(),
            base_url: String::new(),
            api_key: None,
            model_name: String::new(),
            cloud_provider_name: None,
            cloud_model_name: None,
            cloud_api_key: None,
            cloud_base_url: None,
            improv_enabled: false,
            max_follow_up_turns: 2,
            idle_banter_after_secs: 25,
            auto_pause_after_secs: 60,
            category_provider: Default::default(),
            category_model: Default::default(),
            category_api_key: Default::default(),
            category_base_url: Default::default(),
            inference_profile_override: Default::default(),
            category_inference_profile: Default::default(),
            flags: parish_core::config::FeatureFlags::default(),
            category_rate_limit: Default::default(),
            active_tile_source: String::new(),
            tile_sources: Vec::new(),
            reveal_unexplored_locations: false,
            auto_setup_model: None,
        }
    }

    /// `build_app_state` carries the supplied parts onto the constructed
    /// `AppState` — pins the named-field plumbing after the TD-037 refactor
    /// from a 17-arg positional constructor to the `AppStateParts` struct.
    #[test]
    fn build_app_state_carries_parts_onto_state() {
        let saves_dir = PathBuf::from("/tmp/parish-td037-saves");
        let data_dir = PathBuf::from("/tmp/parish-td037-data");
        let flags_path = data_dir.join("parish-flags.json");
        let session_store: Arc<dyn SessionStore> = Arc::new(
            crate::session_store_impl::DbSessionStore::new(saves_dir.clone()),
        );

        let state = build_app_state(AppStateParts {
            session_id: "td037-session".to_string(),
            world: WorldState::new(),
            npc_manager: NpcManager::new(),
            client: None,
            config: test_game_config(),
            cloud_client: None,
            transport: TransportConfig::default(),
            ui_config: UiConfigSnapshot {
                hints_label: String::new(),
                default_accent: "#000".to_string(),
                splash_text: String::new(),
                active_tile_source: String::new(),
                tile_sources: Vec::new(),
                auto_pause_timeout_seconds: 300,
                app_icon_url: None,
                favicon_url: None,
                map_overlay: None,
                base_mod_required: false,
            },
            theme_palette: parish_core::game_mod::default_theme_palette(),
            saves_dir: saves_dir.clone(),
            data_dir: data_dir.clone(),
            game_mod: None,
            flags_path: flags_path.clone(),
            inference_config: InferenceConfig::default(),
            session_store,
            inference_file_log: parish_core::inference::file_log::InferenceFileLog::disabled(),
            chat_transcript_log: parish_core::chat_transcript::ChatTranscriptLog::disabled(),
        });

        assert_eq!(state.session_id, "td037-session");
        assert_eq!(state.saves_dir, saves_dir);
        assert_eq!(state.data_dir, data_dir);
        assert_eq!(state.flags_path, flags_path);
    }

    /// #1366 §2 — every metered coordination lock reports a snapshot, and
    /// the snapshot names cover every node in `LOCK_ORDER` (group nodes via
    /// their dotted member names).
    #[tokio::test]
    async fn lock_metrics_cover_the_lock_order_chain() {
        let saves_dir = PathBuf::from("/tmp/parish-lock-metrics-saves");
        let session_store: Arc<dyn SessionStore> = Arc::new(
            crate::session_store_impl::DbSessionStore::new(saves_dir.clone()),
        );
        let state = build_app_state(AppStateParts {
            session_id: "lock-metrics".to_string(),
            world: WorldState::new(),
            npc_manager: NpcManager::new(),
            client: None,
            config: test_game_config(),
            cloud_client: None,
            transport: TransportConfig::default(),
            ui_config: UiConfigSnapshot {
                hints_label: String::new(),
                default_accent: "#000".to_string(),
                splash_text: String::new(),
                active_tile_source: String::new(),
                tile_sources: Vec::new(),
                auto_pause_timeout_seconds: 300,
                app_icon_url: None,
                favicon_url: None,
                map_overlay: None,
                base_mod_required: false,
            },
            theme_palette: parish_core::game_mod::default_theme_palette(),
            saves_dir: saves_dir.clone(),
            data_dir: PathBuf::from("/tmp/parish-lock-metrics-data"),
            game_mod: None,
            flags_path: PathBuf::from("/tmp/parish-lock-metrics-flags.json"),
            inference_config: InferenceConfig::default(),
            session_store,
            inference_file_log: parish_core::inference::file_log::InferenceFileLog::disabled(),
            chat_transcript_log: parish_core::chat_transcript::ChatTranscriptLog::disabled(),
        });

        let _ = state.world.lock().await;
        let metrics = state.lock_metrics();

        // Every LOCK_ORDER node is represented by at least one snapshot whose
        // name is the node itself or a dotted member of it.
        for node in LOCK_ORDER {
            // `inference_log` is a shared parish-inference type, not a
            // MeteredMutex — it cannot be metered without changing the
            // shared alias.
            if *node == "inference_log" {
                continue;
            }
            assert!(
                metrics
                    .iter()
                    .any(|m| m.name == *node || m.name.starts_with(&format!("{node}."))),
                "LOCK_ORDER node '{node}' has no metrics snapshot"
            );
        }
        let world = metrics.iter().find(|m| m.name == "world").unwrap();
        assert_eq!(world.acquisitions, 1);
    }

    #[tokio::test]
    async fn task_journal_target_cannot_tear_during_save_switch() {
        let identity = Arc::new(SaveIdentity {
            save_path: MeteredMutex::new(
                "save_identity.save_path",
                Some(PathBuf::from("/saves/old.db")),
            ),
            current_branch_id: MeteredMutex::new("save_identity.current_branch_id", Some(1)),
            current_branch_name: MeteredMutex::new(
                "save_identity.current_branch_name",
                Some("old".to_string()),
            ),
        });
        let (path_changed_tx, path_changed_rx) = tokio::sync::oneshot::channel();
        let (finish_switch_tx, finish_switch_rx) = tokio::sync::oneshot::channel();
        let writer_identity = Arc::clone(&identity);
        let writer = tokio::spawn(async move {
            let mut path = writer_identity.save_path.lock().await;
            *path = Some(PathBuf::from("/saves/new.db"));
            path_changed_tx.send(()).unwrap();
            finish_switch_rx.await.unwrap();
            let mut branch_id = writer_identity.current_branch_id.lock().await;
            let mut branch_name = writer_identity.current_branch_name.lock().await;
            *branch_id = Some(2);
            *branch_name = Some("new".to_string());
        });
        path_changed_rx.await.unwrap();

        let reader_identity = Arc::clone(&identity);
        let mut reader = tokio::spawn(async move {
            reader_identity
                .task_journal_target("session")
                .await
                .unwrap()
        });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(20), &mut reader)
                .await
                .is_err(),
            "target capture must wait while a save switch holds the path guard"
        );

        finish_switch_tx.send(()).unwrap();
        writer.await.unwrap();
        let target = reader.await.unwrap();
        assert_eq!(target.save_path, PathBuf::from("/saves/new.db"));
        assert_eq!(target.branch_id, 2);
    }
}
