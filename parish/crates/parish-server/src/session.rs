//! Per-visitor session management.
//!
//! Each visitor gets an isolated [`AppState`], identified by a UUID stored in
//! a `parish_sid` cookie.  Sessions survive server restarts because they are
//! persisted in `saves/sessions.db` and their game state lives in
//! `saves/<session_id>/parish_NNN.db`.
//!
//! # Module layout
//!
//! | Submodule             | Responsibility                                              |
//! |-----------------------|-------------------------------------------------------------|
//! | `persistence`         | [`SessionRegistry`], DB CRUD, OAuth linking, purge          |
//! | `lifecycle`           | `get_or_create_session`, create, restore, finalize          |
//! | `inference_setup`     | client construction, queue init, initial save               |
//! | `ticks`               | background tick tasks (world, autosave, Tier-2, logs, …)   |
//!
//! The public API exported from this module is unchanged from the monolithic
//! `session.rs` that preceded the split (TD-039, #1200).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

use lru::LruCache;
use tokio::sync::Mutex as TokioMutex;
use tokio::task::JoinHandle;

use parish_core::config::InferenceConfig;
use parish_core::game_mod::{GameMod, PronunciationEntry};
use parish_core::ipc::{GameConfig, ThemePalette};
use parish_core::world::transport::TransportConfig;

use parish_core::identity::IdentityStore;

use crate::state::{AppState, UiConfigSnapshot};

// ── Submodules ────────────────────────────────────────────────────────────────

mod inference_setup;
mod lifecycle;
mod persistence;
mod ticks;

// ── Public re-exports ─────────────────────────────────────────────────────────

pub use lifecycle::{CapacityExceededError, get_or_create_session};
pub use persistence::{SessionRegistry, is_valid_session_id};
// Re-export the autosave constant so the test in session/ticks.rs references
// the canonical value without a separate parish_core import.
pub use parish_core::AUTOSAVE_INTERVAL_SECS;

// ── Idempotency cache ─────────────────────────────────────────────────────────

use std::time::Instant;

/// Default capacity of the idempotency LRU cache (process-wide).
pub const IDEMPOTENCY_CACHE_CAPACITY: usize = 1000;

/// Default TTL for idempotency cache entries (24 hours).
pub const IDEMPOTENCY_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// A cached response from a mutating route, stored for idempotency replay.
///
/// The entire serialised body is kept so the replay is byte-identical.  The
/// [`Instant`] records when the entry was inserted so expired entries can be
/// rejected on read without a background sweep.
#[derive(Clone)]
pub struct CachedResponse {
    /// HTTP status code of the original response.
    pub status: u16,
    /// Serialised JSON body of the original response (or empty vec for
    /// status-only responses such as `204 No Content`).
    pub body: Vec<u8>,
    /// Content-Type header value of the original response.
    pub content_type: Option<String>,
    /// Wall-clock instant when this entry was inserted.
    pub inserted_at: Instant,
}

/// Key type for the idempotency cache: `(session_id, idempotency_key)`.
///
/// `session_id` scopes keys to a single visitor so one user's idempotency
/// keys cannot collide with another's.
pub type IdempotencyKey = (String, String);

/// Process-wide LRU cache for idempotent responses.
///
/// Wrapped in a `TokioMutex` so async handlers can hold the guard across
/// `await` points without parking a Tokio worker thread.  Capacity is bounded
/// by [`IDEMPOTENCY_CACHE_CAPACITY`]; the oldest entry is evicted on overflow.
pub type IdempotencyCache = TokioMutex<LruCache<IdempotencyKey, CachedResponse>>;

/// Google OAuth credentials (optional — feature disabled when absent).
pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    /// Public base URL of the server, e.g. `https://parish.example.com`.
    /// Used to construct the OAuth redirect URI.
    pub base_url: String,
}

/// Server-wide state shared by all sessions — one instance per process.
pub struct GlobalState {
    /// Atomic schema-v2 config and transport publication for session admission.
    pub inference_runtime_v2:
        Option<std::sync::Arc<parish_core::inference_runtime_v2::InferenceRuntimeManagerV2>>,
    /// All active sessions, backed by `saves/sessions.db`.
    pub sessions: SessionRegistry,
    /// Identity store — maps OAuth provider identities to stable `account_id`
    /// UUIDs and persists new accounts on first auth (#618).
    pub identity_store: std::sync::Arc<dyn IdentityStore>,
    /// Google OAuth config; `None` disables the login flow.
    pub oauth_config: Option<OAuthConfig>,
    /// Directory containing game data files (`world.json`, `npcs.json`, …).
    pub data_dir: PathBuf,
    /// Resolved path to the world file (`parish.json` or `world.json`).
    pub world_path: PathBuf,
    /// Root saves directory (`saves/`).
    pub saves_dir: PathBuf,
    /// Loaded game mod (for themes, reaction templates, pronunciations).
    pub game_mod: Option<GameMod>,
    /// Pronunciation entries extracted from the game mod.
    pub pronunciations: Vec<PronunciationEntry>,
    /// UI config (splash text, hints label, accent colour).
    pub ui_config: UiConfigSnapshot,
    /// Fixed UI theme palette.
    pub theme_palette: ThemePalette,
    /// Transport mode configuration.
    pub transport: TransportConfig,
    /// Template game config cloned into each new session.
    pub template_config: GameConfig,
    /// TOML-configured inference timeouts loaded from `parish.toml` at boot.
    /// Threaded to every `build_client` call so runtime rebuilds (e.g. after
    /// `/provider`) honour the operator-configured values instead of falling
    /// back to the compiled-in defaults. (#417)
    pub inference_config: InferenceConfig,
    /// Local runtime child processes (Ollama or N vllm-mlx slots).
    /// Held for the server's lifetime so dropping `GlobalState` stops them.
    /// Wrapped in a `Mutex` so the struct stays `Sync`.
    pub runtime_processes: tokio::sync::Mutex<parish_core::inference::client::RuntimeProcesses>,
    /// Disk-backed HTTP cache for NLS historic map tiles.
    ///
    /// Shared across all sessions — tiles are content-addressable (z/x/y) and
    /// session-agnostic, so a single process-wide cache is correct and avoids
    /// duplicating downloads across sessions.  Cache dir resolved once at boot
    /// from `PARISH_TILE_CACHE_DIR` or `<saves_dir>/tile-cache/`.
    pub tile_cache: parish_core::tile_cache::TileCache,
    /// Process-wide idempotency cache for mutating HTTP routes (#619).
    ///
    /// Keyed by `(session_id, Idempotency-Key header value)`.  The LRU
    /// capacity is [`IDEMPOTENCY_CACHE_CAPACITY`]; entries expire after
    /// [`IDEMPOTENCY_TTL`] (24 h).  When the `idempotency-key` feature flag
    /// is disabled this cache is still initialised but never consulted.
    pub idempotency_cache: IdempotencyCache,
    /// Admission-control ceiling: maximum number of concurrent in-memory
    /// sessions per process.  Resolved at boot from `PARISH_MAX_SESSIONS`
    /// env var (preferred) or the `[engine.session]` TOML field; defaults to
    /// 50.  When `None` the `admission-control` feature flag is disabled and
    /// no cap is enforced.
    pub max_concurrent_sessions: Option<usize>,
}

/// A single visitor's isolated game session.
pub struct SessionEntry {
    /// The game state for this visitor.
    pub app_state: Arc<AppState>,
    /// Unix timestamp of the last API request from this session.
    pub last_active: AtomicU64,
    /// Cancellation token — cancelled when the session is evicted so background
    /// tick tasks shut down gracefully instead of running until the JoinHandle
    /// is aborted (#228).
    _shutdown_token: tokio_util::sync::CancellationToken,
    /// Background tick task handles — dropped when the session is evicted.
    _tick_handles: Vec<JoinHandle<()>>,
}

impl Drop for SessionEntry {
    fn drop(&mut self) {
        // Signal all background tasks to stop gracefully before their handles
        // are dropped.  Tasks observe the token in their select! loops and exit
        // cleanly, completing any in-flight autosave iteration first (#228).
        self._shutdown_token.cancel();
    }
}

// ── Module-level constant (re-exported for other modules) ────────────────────

/// Maximum number of gossip propagations performed on a single world tick.
///
/// With many locations and large tier-2 groups a naive "propagate at every
/// group" pass can run hundreds of `propagate_gossip_at_location` calls per
/// tick, stalling the 5-second world tick visibly for all connected
/// clients (#466). Budgeting keeps each tick cheap; remaining groups get
/// picked up by the next tick via a round-robin cursor.
pub(crate) const GOSSIP_BUDGET_PER_TICK: usize = 20;
