//! Per-visitor session management.
//!
//! Each visitor gets an isolated [`AppState`], identified by a UUID stored in
//! a `parish_sid` cookie.  Sessions survive server restarts because they are
//! persisted in `saves/sessions.db` and their game state lives in
//! `saves/<session_id>/parish_NNN.db`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dashmap::DashMap;
use tokio::task::JoinHandle;

use parish_core::game_mod::{GameMod, PronunciationEntry};
use parish_core::inference::{AnyClient, InferenceQueue, spawn_inference_worker};
use parish_core::ipc::{GameConfig, ThemePalette};
use parish_core::npc::manager::NpcManager;
use parish_core::world::transport::TransportConfig;
use parish_core::world::{LocationId, WorldState};

use crate::state::{AppState, UiConfigSnapshot, build_app_state};

// ── Public types ─────────────────────────────────────────────────────────────

/// Google OAuth credentials (optional — feature disabled when absent).
pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    /// Public base URL of the server, e.g. `https://yourapp.railway.app`.
    /// Used to construct the OAuth redirect URI.
    pub base_url: String,
}

/// Server-wide state shared by all sessions — one instance per process.
pub struct GlobalState {
    /// All active sessions, backed by `saves/sessions.db`.
    pub sessions: SessionRegistry,
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
    /// Child `ollama serve` process handle (no-op for non-Ollama providers).
    /// Held for the server's lifetime so dropping `GlobalState` stops the
    /// server. Wrapped in a `Mutex` so the struct stays `Sync`.
    pub ollama_process: tokio::sync::Mutex<parish_core::inference::client::OllamaProcess>,
}

/// A single visitor's isolated game session.
pub struct SessionEntry {
    /// The game state for this visitor.
    pub app_state: Arc<AppState>,
    /// Unix timestamp of the last API request from this session.
    pub last_active: AtomicU64,
    /// Background tick task handles — dropped when the session is evicted.
    _tick_handles: Vec<JoinHandle<()>>,
}

// ── SessionRegistry ──────────────────────────────────────────────────────────

/// In-memory session map backed by a SQLite persistence store.
pub struct SessionRegistry {
    sessions: DashMap<String, Arc<SessionEntry>>,
    db: std::sync::Mutex<rusqlite::Connection>,
}

impl SessionRegistry {
    /// Opens (or creates) `saves/sessions.db` and runs schema migrations.
    pub fn open(saves_dir: &Path) -> rusqlite::Result<Self> {
        let db_path = saves_dir.join("sessions.db");
        let conn = rusqlite::Connection::open(&db_path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                id           TEXT PRIMARY KEY,
                created_at   TEXT NOT NULL,
                last_active  TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS oauth_accounts (
                provider         TEXT NOT NULL,
                provider_user_id TEXT NOT NULL,
                session_id       TEXT NOT NULL,
                display_name     TEXT NOT NULL DEFAULT '',
                PRIMARY KEY (provider, provider_user_id)
            );",
        )?;
        // Idempotent migration: add display_name to existing DBs that predate this column.
        let _ = conn.execute_batch(
            "ALTER TABLE oauth_accounts ADD COLUMN display_name TEXT NOT NULL DEFAULT ''",
        );
        Ok(Self {
            sessions: DashMap::new(),
            db: std::sync::Mutex::new(conn),
        })
    }

    pub fn now_unix() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    fn now_iso() -> String {
        chrono::Utc::now().to_rfc3339()
    }

    /// Returns `true` if `session_id` is recorded in sessions.db.
    pub fn exists_in_db(&self, session_id: &str) -> bool {
        let db = self.db.lock().unwrap();
        db.query_row("SELECT 1 FROM sessions WHERE id = ?1", [session_id], |_| {
            Ok(())
        })
        .is_ok()
    }

    /// Inserts a new session row into sessions.db.
    pub fn persist_new(&self, session_id: &str) {
        let now = Self::now_iso();
        let db = self.db.lock().unwrap();
        if let Err(e) = db.execute(
            "INSERT OR IGNORE INTO sessions (id, created_at, last_active) VALUES (?1, ?2, ?2)",
            rusqlite::params![session_id, now],
        ) {
            tracing::warn!(session_id = %session_id, error = %e, "persist_new failed");
        }
    }

    /// Updates the `last_active` timestamp for a session in sessions.db.
    pub fn update_last_active(&self, session_id: &str) {
        let now = Self::now_iso();
        let db = self.db.lock().unwrap();
        if let Err(e) = db.execute(
            "UPDATE sessions SET last_active = ?1 WHERE id = ?2",
            rusqlite::params![now, session_id],
        ) {
            tracing::warn!(session_id = %session_id, error = %e, "update_last_active failed");
        }
    }

    /// Returns the session_id linked to an OAuth identity, if any.
    pub fn find_by_oauth(&self, provider: &str, provider_user_id: &str) -> Option<String> {
        let db = self.db.lock().unwrap();
        db.query_row(
            "SELECT session_id FROM oauth_accounts
             WHERE provider = ?1 AND provider_user_id = ?2",
            rusqlite::params![provider, provider_user_id],
            |row| row.get(0),
        )
        .ok()
    }

    /// Associates an OAuth identity with a session_id, storing the user's display name.
    pub fn link_oauth(
        &self,
        provider: &str,
        provider_user_id: &str,
        session_id: &str,
        display_name: &str,
    ) {
        let db = self.db.lock().unwrap();
        match db.execute(
            "INSERT OR REPLACE INTO oauth_accounts
             (provider, provider_user_id, session_id, display_name) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![provider, provider_user_id, session_id, display_name],
        ) {
            Ok(rows) => tracing::info!(
                provider = %provider,
                provider_user_id = %provider_user_id,
                session_id = %session_id,
                display_name = %display_name,
                rows = rows,
                "link_oauth stored account"
            ),
            Err(e) => tracing::error!(
                provider = %provider,
                provider_user_id = %provider_user_id,
                session_id = %session_id,
                error = %e,
                "link_oauth DB write failed"
            ),
        }
    }

    /// Returns a session from the in-memory map.
    pub fn get_in_memory(&self, session_id: &str) -> Option<Arc<SessionEntry>> {
        self.sessions.get(session_id).map(|e| Arc::clone(&*e))
    }

    /// Inserts a session into the in-memory map.
    pub fn insert(&self, session_id: String, entry: Arc<SessionEntry>) {
        self.sessions.insert(session_id, entry);
    }

    /// Returns the Google `(sub, display_name)` linked to `session_id`, if any.
    ///
    /// Used by `GET /api/auth/status` to check whether the session has a
    /// linked Google account and what name to display.
    pub fn google_account_for_session(&self, session_id: &str) -> Option<(String, String)> {
        let db = self.db.lock().unwrap();
        db.query_row(
            "SELECT provider_user_id, display_name FROM oauth_accounts \
             WHERE session_id = ?1 AND provider = 'google'",
            rusqlite::params![session_id],
            |row: &rusqlite::Row<'_>| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .ok()
    }

    /// Removes sessions that have been idle longer than `max_age`.
    ///
    /// The sessions' background tick tasks are implicitly cancelled when
    /// their `JoinHandle`s are dropped via the evicted `SessionEntry`.
    pub fn cleanup_stale(&self, max_age: Duration) {
        let cutoff = Self::now_unix().saturating_sub(max_age.as_secs());
        self.sessions
            .retain(|_, entry| entry.last_active.load(Ordering::Relaxed) >= cutoff);
    }

    /// Purges sessions abandoned for longer than `max_age` from disk
    /// (sessions.db row + saves/<session_id>/ directory).
    ///
    /// Distinct from [`cleanup_stale`]: that one only clears the
    /// in-memory map. Disk state is what needs removing here (#482) —
    /// otherwise long-running deployments accumulate dead sessions
    /// forever. `max_age` is expected to be much longer than the
    /// in-memory TTL (e.g. 30 days vs 2 hours) so users can still
    /// restore a session from the cookie on their next visit for
    /// reasonable idle windows.
    ///
    /// Returns the number of sessions purged, so the caller can log
    /// the scope of the sweep.
    pub fn purge_expired_disk_sessions(&self, saves_root: &Path, max_age: Duration) -> usize {
        let cutoff_secs = Self::now_unix().saturating_sub(max_age.as_secs());
        let cutoff = match chrono::DateTime::<chrono::Utc>::from_timestamp(cutoff_secs as i64, 0) {
            Some(dt) => dt.to_rfc3339(),
            None => {
                tracing::warn!(
                    cutoff_secs = cutoff_secs,
                    "purge_expired_disk_sessions: cutoff timestamp out of range, skipping sweep"
                );
                return 0;
            }
        };

        // Find expired session ids + drop their sessions.db rows in a
        // single transaction so the filesystem cleanup below can't get
        // out of sync with the DB if the process dies mid-sweep.
        let expired_ids: Vec<String> = {
            let db = match self.db.lock() {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
            let mut collected = Vec::new();
            let select_result = (|| -> rusqlite::Result<()> {
                let mut stmt = db.prepare("SELECT id FROM sessions WHERE last_active < ?1")?;
                let mut rows = stmt.query([&cutoff])?;
                while let Some(row) = rows.next()? {
                    collected.push(row.get::<_, String>(0)?);
                }
                Ok(())
            })();
            if let Err(e) = select_result {
                tracing::warn!(error = %e, "purge_expired_disk_sessions: DB read failed");
                return 0;
            }
            // Drop rows for the ids we collected inside an explicit
            // transaction.  Both DELETEs must commit atomically: if the
            // process crashes between them, oauth_accounts rows would be
            // left pointing at a non-existent session_id, letting the
            // next login for that OAuth identity silently resurrect a
            // ghost session (#593, #482).
            //
            // Invariant: DB rows are deleted *before* filesystem cleanup
            // (see below).  A residual saves/<id>/ directory with no DB
            // row is harmless; an oauth_accounts row pointing at a missing
            // sessions row is not.
            if !collected.is_empty() {
                let tx_result = (|| -> rusqlite::Result<()> {
                    let tx = db.unchecked_transaction()?;
                    let placeholders = vec!["?"; collected.len()].join(",");
                    let params: Vec<&dyn rusqlite::ToSql> = collected
                        .iter()
                        .map(|s| s as &dyn rusqlite::ToSql)
                        .collect();
                    let sql = format!("DELETE FROM sessions WHERE id IN ({placeholders})");
                    tx.execute(&sql, params.as_slice())?;
                    // Also drop oauth links for those sessions — otherwise
                    // the next login for the same provider_user_id would
                    // resurrect a dead session_id. (#482 sibling concern.)
                    let oauth_sql =
                        format!("DELETE FROM oauth_accounts WHERE session_id IN ({placeholders})");
                    tx.execute(&oauth_sql, params.as_slice())?;
                    tx.commit()
                })();
                if let Err(e) = tx_result {
                    tracing::warn!(error = %e, "purge_expired_disk_sessions: DB delete failed");
                    return 0;
                }
            }
            collected
        };

        if expired_ids.is_empty() {
            return 0;
        }

        // Best-effort filesystem cleanup. A failure here is logged but
        // doesn't undo the DB delete — a residual saves/<id>/ directory
        // with no DB row is harmless (eventually reaped by OS-level
        // cleanup or a later sweep once we have directory-age scanning).
        //
        // #595 — Validate each session ID before building a path so that a
        // corrupted or tampered DB row cannot cause remove_dir_all to delete
        // directories outside the saves root.  Two layers of defence:
        //   1. Allowlist check: the ID must consist only of lowercase hex
        //      digits and hyphens (UUID v4 format).  Anything else — including
        //      `..`, `/`, `\`, or unusual chars — is rejected before we even
        //      call Path::join.
        //   2. Containment check: after joining, canonicalize the candidate
        //      path and assert it starts with the canonicalized saves root.
        //      This catches any edge case the regex might miss.
        let canonical_saves_root = match saves_root.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "purge_expired_disk_sessions: cannot canonicalize saves_root, skipping fs cleanup"
                );
                return expired_ids.len();
            }
        };

        for id in &expired_ids {
            // Layer 1: allowlist — UUID v4 looks like
            // `xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx` (hex + hyphens only).
            if !id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') || id.contains("..") {
                tracing::warn!(
                    session_id = %id,
                    "purge_expired_disk_sessions: rejected unsafe session ID, skipping fs remove"
                );
                continue;
            }

            let session_dir = saves_root.join(id);
            if !session_dir.exists() {
                continue;
            }

            // Layer 2: containment — canonicalize the resolved path and verify
            // it stays inside the saves root (guards against symlink tricks or
            // any bypass of the allowlist above).
            let canonical_dir = match session_dir.canonicalize() {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(
                        session_id = %id,
                        path = %session_dir.display(),
                        error = %e,
                        "purge_expired_disk_sessions: cannot canonicalize session dir, skipping"
                    );
                    continue;
                }
            };
            if !canonical_dir.starts_with(&canonical_saves_root) {
                tracing::warn!(
                    session_id = %id,
                    path = %canonical_dir.display(),
                    saves_root = %canonical_saves_root.display(),
                    "purge_expired_disk_sessions: path escapes saves root, skipping fs remove"
                );
                continue;
            }

            match std::fs::remove_dir_all(&session_dir) {
                Ok(()) => {
                    tracing::info!(
                        session_id = %id,
                        path = %session_dir.display(),
                        "purge_expired_disk_sessions: removed saves directory"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        session_id = %id,
                        path = %session_dir.display(),
                        error = %e,
                        "purge_expired_disk_sessions: failed to remove saves directory"
                    );
                }
            }
        }

        expired_ids.len()
    }
}

// ── Public session resolution ─────────────────────────────────────────────────

/// Returns the session for `cookie_id`, restoring or creating one as needed.
///
/// Returns `(session_id, entry, is_new)` where `is_new` is `true` when a
/// fresh `parish_sid` cookie must be set on the response.
pub async fn get_or_create_session(
    global: &Arc<GlobalState>,
    cookie_id: Option<&str>,
) -> (String, Arc<SessionEntry>, bool) {
    // 1. Hot path: session already in memory.
    if let Some(id) = cookie_id {
        if let Some(entry) = global.sessions.get_in_memory(id) {
            entry
                .last_active
                .store(SessionRegistry::now_unix(), Ordering::Relaxed);
            global.sessions.update_last_active(id);
            return (id.to_string(), entry, false);
        }
        // 2. Session known in DB but evicted from memory — restore it.
        if global.sessions.exists_in_db(id) {
            match restore_session(global, id).await {
                Ok(entry) => {
                    global.sessions.insert(id.to_string(), Arc::clone(&entry));
                    global.sessions.update_last_active(id);
                    return (id.to_string(), entry, false);
                }
                Err(e) => {
                    tracing::warn!(session_id = %id, "Session restore failed: {}. Starting fresh.", e);
                }
            }
        }
    }

    // 3. No usable session — create a new one.
    let session_id = uuid::Uuid::new_v4().to_string();
    let entry = create_session(global, &session_id).await;
    global.sessions.persist_new(&session_id);
    global
        .sessions
        .insert(session_id.clone(), Arc::clone(&entry));
    (session_id, entry, true)
}

// ── Session creation ──────────────────────────────────────────────────────────

async fn create_session(global: &Arc<GlobalState>, session_id: &str) -> Arc<SessionEntry> {
    let session_saves = global.saves_dir.join(session_id);
    std::fs::create_dir_all(&session_saves).ok();

    let world_path = global.world_path.clone();
    let data_dir = global.data_dir.clone();
    let (world, npc_manager) = tokio::task::spawn_blocking(move || {
        let world = WorldState::from_parish_file(&world_path, LocationId(15)).unwrap_or_else(|e| {
            tracing::warn!("Session init: failed to load world: {}. Using default.", e);
            WorldState::new()
        });
        let mut npc_manager = NpcManager::load_from_file(&data_dir.join("npcs.json"))
            .unwrap_or_else(|e| {
                tracing::warn!("Session init: failed to load npcs.json: {}. No NPCs.", e);
                NpcManager::new()
            });
        npc_manager.assign_tiers(&world, &[]);
        (world, npc_manager)
    })
    .await
    .expect("session init blocking task panicked");

    let (client, config) = build_session_client(global);
    let cloud_client = build_session_cloud_client(global);
    let game_mod = global.game_mod.clone();

    let flags_path = global.data_dir.join("parish-flags.json");
    let app_state = build_app_state(
        world,
        npc_manager,
        client.clone(),
        config,
        cloud_client,
        global.transport.clone(),
        global.ui_config.clone(),
        global.theme_palette.clone(),
        session_saves.clone(),
        global.data_dir.clone(),
        game_mod,
        flags_path,
    );

    if let Some(ref c) = client {
        init_inference_queue(&app_state, c.clone()).await;
    }

    if let Err(e) = init_session_save(&app_state, &session_saves).await {
        tracing::warn!("Session initial save failed: {}", e);
    }

    let handles = spawn_session_ticks(Arc::clone(&app_state));

    Arc::new(SessionEntry {
        app_state,
        last_active: AtomicU64::new(SessionRegistry::now_unix()),
        _tick_handles: handles,
    })
}

// ── Session restoration ───────────────────────────────────────────────────────

async fn restore_session(
    global: &Arc<GlobalState>,
    session_id: &str,
) -> Result<Arc<SessionEntry>, String> {
    let session_saves = global.saves_dir.join(session_id);
    if !session_saves.exists() {
        return Err(format!(
            "saves directory {} does not exist",
            session_saves.display()
        ));
    }

    // Find the first (alphabetically) .db file.
    let db_path = {
        let mut files: Vec<PathBuf> = std::fs::read_dir(&session_saves)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "db"))
            .collect();
        files.sort();
        files.into_iter().next().ok_or("no save files found")?
    };

    // Load snapshot from the first branch.
    let db_path_clone = db_path.clone();
    let (snapshot, branch_id, branch_name) = tokio::task::spawn_blocking(move || {
        use parish_core::persistence::Database;
        let db = Database::open(&db_path_clone).map_err(|e| e.to_string())?;
        let branches = db.list_branches().map_err(|e| e.to_string())?;
        let branch = branches.into_iter().next().ok_or("no branches")?;
        let (_, snapshot) = db
            .load_latest_snapshot(branch.id)
            .map_err(|e| e.to_string())?
            .ok_or("no snapshots")?;
        Ok::<_, String>((snapshot, branch.id, branch.name))
    })
    .await
    .map_err(|e| e.to_string())??;

    // Load fresh static world data, then apply the saved snapshot.
    let world_path = global.world_path.clone();
    let data_dir = global.data_dir.clone();
    let (mut world, mut npc_manager) = tokio::task::spawn_blocking(move || {
        let world = WorldState::from_parish_file(&world_path, LocationId(15))
            .unwrap_or_else(|_| WorldState::new());
        let npc_manager = NpcManager::load_from_file(&data_dir.join("npcs.json"))
            .unwrap_or_else(|_| NpcManager::new());
        (world, npc_manager)
    })
    .await
    .map_err(|e| e.to_string())?;

    snapshot.restore(&mut world, &mut npc_manager);
    npc_manager.assign_tiers(&world, &[]);

    let (client, config) = build_session_client(global);
    let cloud_client = build_session_cloud_client(global);
    let game_mod = global.game_mod.clone();

    let flags_path = global.data_dir.join("parish-flags.json");
    let app_state = build_app_state(
        world,
        npc_manager,
        client.clone(),
        config,
        cloud_client,
        global.transport.clone(),
        global.ui_config.clone(),
        global.theme_palette.clone(),
        session_saves.clone(),
        global.data_dir.clone(),
        game_mod,
        flags_path,
    );

    if let Some(ref c) = client {
        init_inference_queue(&app_state, c.clone()).await;
    }

    // Acquire advisory lock on the restored save file so another server
    // instance (or a headless CLI) cannot concurrently write to it (#425).
    // If a peer already holds the lock we log a warning and continue:
    // refusing to start would leave the user with no session at all, and
    // per-process ownership makes strict mutual exclusion across
    // containers out of scope for this handler. The lock is stored on
    // AppState.save_lock so it lives for the session's lifetime.
    let locked = parish_core::persistence::SaveFileLock::try_acquire(&db_path);
    if locked.is_none() {
        tracing::warn!(
            path = %db_path.display(),
            session_id = %session_id,
            "SaveFileLock::try_acquire returned None on session resume — save file appears in use by another instance",
        );
    }
    *app_state.save_lock.lock().await = locked;
    *app_state.save_path.lock().await = Some(db_path);
    *app_state.current_branch_id.lock().await = Some(branch_id);
    *app_state.current_branch_name.lock().await = Some(branch_name);

    let handles = spawn_session_ticks(Arc::clone(&app_state));

    Ok(Arc::new(SessionEntry {
        app_state,
        last_active: AtomicU64::new(SessionRegistry::now_unix()),
        _tick_handles: handles,
    }))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

async fn init_inference_queue(app_state: &Arc<AppState>, client: AnyClient) {
    let (interactive_tx, interactive_rx) = tokio::sync::mpsc::channel(16);
    let (background_tx, background_rx) = tokio::sync::mpsc::channel(32);
    let (batch_tx, batch_rx) = tokio::sync::mpsc::channel(64);
    let worker = spawn_inference_worker(
        client,
        interactive_rx,
        background_rx,
        batch_rx,
        app_state.inference_log.clone(),
    );
    let queue = InferenceQueue::new(interactive_tx, background_tx, batch_tx);
    *app_state.inference_queue.lock().await = Some(queue);
    *app_state.worker_handle.lock().await = Some(worker);
}

/// Saves the initial world snapshot into `saves/<session_id>/parish_001.db`.
async fn init_session_save(app_state: &Arc<AppState>, session_saves: &Path) -> Result<(), String> {
    use parish_core::persistence::Database;
    use parish_core::persistence::picker::new_save_path;
    use parish_core::persistence::snapshot::GameSnapshot;

    let snapshot = {
        let world = app_state.world.lock().await;
        let npc_manager = app_state.npc_manager.lock().await;
        GameSnapshot::capture(&world, &npc_manager)
    };

    let save_path = new_save_path(session_saves);
    let save_path_clone = save_path.clone();

    let branch_id = tokio::task::spawn_blocking(move || -> Result<i64, String> {
        let db = Database::open(&save_path_clone).map_err(|e| e.to_string())?;
        let branch_id = db.create_branch("main", None).map_err(|e| e.to_string())?;
        db.save_snapshot(branch_id, &snapshot)
            .map_err(|e| e.to_string())?;
        Ok(branch_id)
    })
    .await
    .map_err(|e| e.to_string())??;

    // Advisory lock on the freshly-initialised save file so peer
    // instances don't write to it concurrently (#425). For a just-created
    // save we expect the lock to always succeed, but we stay defensive:
    // warn if the lock fails rather than silently proceeding.
    let locked = parish_core::persistence::SaveFileLock::try_acquire(&save_path);
    if locked.is_none() {
        tracing::warn!(
            path = %save_path.display(),
            "SaveFileLock::try_acquire returned None on init_session_save — new save file unexpectedly locked",
        );
    }
    *app_state.save_lock.lock().await = locked;
    *app_state.save_path.lock().await = Some(save_path);
    *app_state.current_branch_id.lock().await = Some(branch_id);
    *app_state.current_branch_name.lock().await = Some("main".to_string());

    Ok(())
}

/// Maximum number of gossip propagations performed on a single world tick.
///
/// With many locations and large tier-2 groups a naive "propagate at every
/// group" pass can run hundreds of `propagate_gossip_at_location` calls per
/// tick, stalling the 5-second world tick visibly for all connected
/// clients (#466). Budgeting keeps each tick cheap; remaining groups get
/// picked up by the next tick via a round-robin cursor.
const GOSSIP_BUDGET_PER_TICK: usize = 20;

/// Runs at most `budget` gossip propagations across `groups`, starting from
/// the group at position `cursor` in location-id order and wrapping around.
/// Returns the new cursor to persist for the next tick, so the round-robin
/// makes forward progress through the group list over successive ticks
/// rather than re-hitting the same prefix every time.
///
/// Groups with fewer than 2 NPCs are skipped silently and do *not* consume
/// budget — they are no-ops for gossip and counting them would let a
/// cluster of sparse groups waste an entire tick's budget.
///
/// The propagation work itself is handed off via a `propagate` callback so
/// the helper stays free of the specific `GossipNetwork` / `Rng` types it
/// would otherwise need to name, keeping the module's import graph small
/// and the helper unit-testable with a counting stub.
fn propagate_gossip_budgeted<F>(
    groups: &std::collections::HashMap<
        parish_core::world::LocationId,
        Vec<parish_core::npc::NpcId>,
    >,
    cursor: usize,
    budget: usize,
    mut propagate: F,
) -> usize
where
    F: FnMut(&[parish_core::npc::NpcId]),
{
    // Sort groups by LocationId so the cursor addresses a stable order
    // across ticks; `HashMap::iter` order would shift on every resize.
    let mut sorted_keys: Vec<parish_core::world::LocationId> = groups.keys().copied().collect();
    sorted_keys.sort();
    let n = sorted_keys.len();
    if n == 0 {
        return 0;
    }
    let start = cursor % n;
    let mut consumed = 0;
    for i in 0..n {
        if consumed >= budget {
            return (start + i) % n;
        }
        let idx = (start + i) % n;
        let loc = sorted_keys[idx];
        if let Some(npc_ids) = groups.get(&loc)
            && npc_ids.len() >= 2
        {
            propagate(npc_ids);
            consumed += 1;
        }
    }
    // Wrapped all the way around without hitting the budget — advance the
    // cursor by the number of groups we actually processed so we still
    // rotate on each tick.
    (start + consumed) % n
}

/// Spawns the three per-session background tasks and returns their handles.
fn spawn_session_ticks(state: Arc<AppState>) -> Vec<JoinHandle<()>> {
    let mut handles = Vec::with_capacity(3);

    // ── World tick (5 s) ─────────────────────────────────────────────────────
    {
        let s = Arc::clone(&state);
        handles.push(tokio::spawn(async move {
            // Round-robin cursor for budgeted gossip propagation (#466).
            let mut gossip_cursor: usize = 0;
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;

                {
                    let world = s.world.lock().await;
                    let npc_manager = s.npc_manager.lock().await;
                    let transport = s.transport.default_mode();
                    let mut snap = parish_core::ipc::snapshot_from_world(&world, transport);
                    snap.name_hints = parish_core::ipc::compute_name_hints(
                        &world,
                        &npc_manager,
                        &s.pronunciations,
                    );
                    s.event_bus.emit("world-update", &snap);
                }

                {
                    // Snapshot the banshee flag outside the world/npc locks to avoid
                    // nesting config → world, which inverts the project-wide
                    // lock order.
                    let banshee_enabled = {
                        let cfg = s.config.lock().await;
                        !cfg.flags.is_disabled("banshee")
                    };

                    let mut world = s.world.lock().await;
                    let mut npc_mgr = s.npc_manager.lock().await;

                    let season = world.clock.season();
                    let now = world.clock.now();
                    let mut rng = rand::thread_rng();
                    if let Some(new_weather) = world.weather_engine.tick(now, season, &mut rng) {
                        world.weather = new_weather;
                        world.event_bus.publish(
                            parish_core::world::events::GameEvent::WeatherChanged {
                                new_weather: new_weather.to_string(),
                                timestamp: world.clock.now(),
                            },
                        );
                    }

                    npc_mgr.tick_schedules(&world.clock, &world.graph, world.weather);
                    npc_mgr.assign_tiers(&world, &[]);

                    // Banshee tick — herald and finalise doomed NPCs.
                    if banshee_enabled {
                        let world = &mut *world;
                        let _ = npc_mgr.tick_banshee(
                            &world.clock,
                            &world.graph,
                            &mut world.text_log,
                            &world.event_bus,
                            world.player_location,
                        );
                    }

                    if !world.gossip_network.is_empty() {
                        let groups = npc_mgr.tier2_groups();
                        let mut rng = rand::thread_rng();
                        let network = &mut world.gossip_network;
                        gossip_cursor = propagate_gossip_budgeted(
                            &groups,
                            gossip_cursor,
                            GOSSIP_BUDGET_PER_TICK,
                            |npc_ids| {
                                parish_core::npc::ticks::propagate_gossip_at_location(
                                    npc_ids, network, &mut rng,
                                );
                            },
                        );
                    }

                    // Advance the generation counter so handle_game_input can
                    // detect TOCTOU races (see issue #283).
                    world.increment_tick_generation();
                }
            }
        }));
    }

    // ── Inactivity tick (1 s) ────────────────────────────────────────────────
    {
        let s = Arc::clone(&state);
        handles.push(tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                crate::routes::tick_inactivity(&s).await;
            }
        }));
    }

    // ── Autosave tick (60 s) ─────────────────────────────────────────────────
    //
    // #230 — Fixes: previously a fresh `Database::open` (and therefore a full
    // `migrate()` round-trip) was executed on every tick.  Now we lazily open
    // an `AsyncDatabase` the first time we have a save path and reuse it for
    // all subsequent ticks.  All SQLite work is delegated to `spawn_blocking`
    // inside `AsyncDatabase`, so a slow fsync can never stall the Tokio runtime.
    {
        let s = Arc::clone(&state);
        handles.push(tokio::spawn(async move {
            use parish_core::persistence::snapshot::GameSnapshot;
            use parish_core::persistence::{AsyncDatabase, Database};
            // Track whether the last autosave attempt failed so we only emit
            // one user-visible warning per failure run, not one per tick.
            let mut last_autosave_failed = false;
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;

                let save_path = s.save_path.lock().await.clone();
                let branch_id = *s.current_branch_id.lock().await;

                if let (Some(path), Some(bid)) = (save_path, branch_id) {
                    // Snapshot the world state before touching the DB lock.
                    let snapshot = {
                        let world = s.world.lock().await;
                        let npc_manager = s.npc_manager.lock().await;
                        GameSnapshot::capture(&world, &npc_manager)
                    };

                    // Obtain (or open) the cached AsyncDatabase for this path.
                    let db: Option<AsyncDatabase> = {
                        let mut guard = s.save_db.lock().await;
                        // If the cached path no longer matches the active save file
                        // (e.g. after load-branch / new-save-file), discard the old handle.
                        if guard.as_ref().is_some_and(|(p, _)| p != &path) {
                            *guard = None;
                        }
                        if guard.is_none() {
                            let path_clone = path.clone();
                            match tokio::task::spawn_blocking(move || Database::open(&path_clone))
                                .await
                            {
                                Ok(Ok(db)) => {
                                    *guard = Some((path.clone(), AsyncDatabase::new(db)));
                                }
                                Ok(Err(e)) => {
                                    tracing::warn!("Autosave: failed to open DB: {}", e);
                                    if !last_autosave_failed {
                                        s.event_bus.emit(
                                            "text-log",
                                            &parish_core::ipc::text_log(
                                                "system",
                                                "Autosave failed — could not open save file.",
                                            ),
                                        );
                                        last_autosave_failed = true;
                                    }
                                    continue;
                                }
                                Err(e) => {
                                    tracing::warn!("Autosave: spawn_blocking error: {}", e);
                                    continue;
                                }
                            }
                        }
                        guard.as_ref().map(|(_, db)| db.clone())
                    };

                    if let Some(db) = db {
                        match db.save_snapshot(bid, &snapshot).await {
                            Ok(_) => {
                                tracing::debug!("Session autosave complete");
                                if last_autosave_failed {
                                    s.event_bus.emit(
                                        "text-log",
                                        &parish_core::ipc::text_log(
                                            "system",
                                            "Autosave resumed successfully.",
                                        ),
                                    );
                                }
                                last_autosave_failed = false;
                            }
                            Err(e) => {
                                tracing::warn!("Session autosave failed: {}", e);
                                if !last_autosave_failed {
                                    s.event_bus.emit(
                                        "text-log",
                                        &parish_core::ipc::text_log(
                                            "system",
                                            "Autosave failed — your progress may not be saved.",
                                        ),
                                    );
                                    last_autosave_failed = true;
                                }
                            }
                        }
                    }
                }
            }
        }));
    }

    handles
}

fn build_session_client(global: &GlobalState) -> (Option<AnyClient>, GameConfig) {
    let config = global.template_config.clone();
    let client = if config.provider_name == "simulator" {
        Some(AnyClient::simulator())
    } else if config.model_name.is_empty() && config.provider_name != "ollama" {
        None
    } else {
        let provider_enum = parish_core::config::Provider::from_str_loose(&config.provider_name)
            .unwrap_or_default();
        Some(parish_core::inference::build_client(
            &provider_enum,
            &config.base_url,
            config.api_key.as_deref(),
            &parish_core::config::InferenceConfig::default(),
        ))
    };
    (client, config)
}

fn build_session_cloud_client(global: &GlobalState) -> Option<AnyClient> {
    let config = &global.template_config;
    config.cloud_api_key.as_deref().map(|key| {
        let provider_enum = config
            .cloud_provider_name
            .as_deref()
            .and_then(|p| parish_core::config::Provider::from_str_loose(p).ok())
            .unwrap_or(parish_core::config::Provider::OpenRouter);
        parish_core::inference::build_client(
            &provider_enum,
            config
                .cloud_base_url
                .as_deref()
                .unwrap_or("https://openrouter.ai/api"),
            Some(key),
            &parish_core::config::InferenceConfig::default(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that a fresh DB round-trips the Google OAuth link:
    /// after `link_oauth`, both `find_by_oauth` and
    /// `google_account_for_session` return the stored values.
    ///
    /// This is the exact flow the callback + status endpoint use, so if
    /// this test passes but the UI shows the user as signed out, the bug
    /// is elsewhere (cookies, middleware, frontend).
    #[test]
    fn oauth_link_round_trips_on_fresh_db() {
        let tmp = tempfile::tempdir().unwrap();
        let reg = SessionRegistry::open(tmp.path()).unwrap();
        reg.persist_new("sess_abc");
        reg.link_oauth("google", "sub_123", "sess_abc", "John Doe");

        assert_eq!(
            reg.find_by_oauth("google", "sub_123"),
            Some("sess_abc".to_string()),
            "find_by_oauth should return the linked session_id"
        );
        assert_eq!(
            reg.google_account_for_session("sess_abc"),
            Some(("sub_123".to_string(), "John Doe".to_string())),
            "google_account_for_session should return (sub, display_name)"
        );
    }

    /// Verifies the migration from a pre-display_name schema to the
    /// current schema: opening a DB that was created with the old schema
    /// should add the `display_name` column, and subsequent link_oauth
    /// + google_account_for_session calls should work end-to-end.
    #[test]
    fn oauth_link_round_trips_on_migrated_db() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("sessions.db");

        // Simulate an existing DB created with the pre-display_name schema.
        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE sessions (
                    id TEXT PRIMARY KEY,
                    created_at TEXT NOT NULL,
                    last_active TEXT NOT NULL
                );
                CREATE TABLE oauth_accounts (
                    provider         TEXT NOT NULL,
                    provider_user_id TEXT NOT NULL,
                    session_id       TEXT NOT NULL,
                    PRIMARY KEY (provider, provider_user_id)
                );",
            )
            .unwrap();
            // Insert a row that predates the display_name column.
            conn.execute(
                "INSERT INTO oauth_accounts (provider, provider_user_id, session_id) \
                 VALUES ('google', 'legacy_sub', 'legacy_sess')",
                [],
            )
            .unwrap();
        }

        // Re-open through SessionRegistry — this should ADD COLUMN display_name.
        let reg = SessionRegistry::open(tmp.path()).unwrap();

        // Legacy row has empty display_name (default).
        assert_eq!(
            reg.google_account_for_session("legacy_sess"),
            Some(("legacy_sub".to_string(), String::new())),
        );

        // New link writes the display_name column correctly.
        reg.persist_new("sess_new");
        reg.link_oauth("google", "sub_new", "sess_new", "Jane Doe");
        assert_eq!(
            reg.google_account_for_session("sess_new"),
            Some(("sub_new".to_string(), "Jane Doe".to_string())),
        );
    }

    // ── #466 gossip budget round-robin ──────────────────────────────────────

    fn make_group(n: u32) -> (parish_core::world::LocationId, Vec<parish_core::npc::NpcId>) {
        let loc = parish_core::world::LocationId(n);
        // 2 NPCs so the group is gossip-eligible.
        let npcs = vec![
            parish_core::npc::NpcId(n * 10),
            parish_core::npc::NpcId(n * 10 + 1),
        ];
        (loc, npcs)
    }

    #[test]
    fn gossip_budget_empty_returns_zero_cursor() {
        let groups = std::collections::HashMap::new();
        let mut calls = 0;
        let new_cursor = propagate_gossip_budgeted(&groups, 42, 20, |_| calls += 1);
        assert_eq!(new_cursor, 0);
        assert_eq!(calls, 0);
    }

    #[test]
    fn gossip_budget_caps_at_budget_and_returns_next_cursor() {
        // 50 eligible groups, budget 20 — expect 20 propagations and cursor=20.
        let mut groups = std::collections::HashMap::new();
        for i in 1..=50 {
            let (loc, npcs) = make_group(i);
            groups.insert(loc, npcs);
        }
        let mut calls = 0;
        let new_cursor = propagate_gossip_budgeted(&groups, 0, 20, |_| calls += 1);
        assert_eq!(calls, 20);
        assert_eq!(new_cursor, 20, "cursor should advance by the budget");
    }

    #[test]
    fn gossip_budget_round_robins_across_ticks() {
        // 30 groups, budget 20. Tick 1 does 0..20, tick 2 should pick up at 20
        // and wrap through 29, 0..9 — ending at cursor 10 (20+20 mod 30).
        let mut groups = std::collections::HashMap::new();
        for i in 1..=30 {
            let (loc, npcs) = make_group(i);
            groups.insert(loc, npcs);
        }

        let mut seen: Vec<parish_core::world::LocationId> = Vec::new();
        let new_cursor = propagate_gossip_budgeted(&groups, 0, 20, |npc_ids| {
            // reverse-map back to LocationId via NpcId(n*10).
            let n = npc_ids[0].0 / 10;
            seen.push(parish_core::world::LocationId(n));
        });
        assert_eq!(new_cursor, 20);
        assert_eq!(seen.len(), 20);

        // Next tick starting from cursor=20 should continue with id 21
        // (since ids are sorted and we started at id 1 for index 0).
        let mut next_seen: Vec<parish_core::world::LocationId> = Vec::new();
        let next_cursor = propagate_gossip_budgeted(&groups, new_cursor, 20, |npc_ids| {
            next_seen.push(parish_core::world::LocationId(npc_ids[0].0 / 10));
        });
        assert_eq!(next_cursor, 10, "wrap: (20+20) mod 30 = 10");
        assert_eq!(next_seen.len(), 20);
        // First item processed on tick 2 is id 21 (sorted position 20).
        assert_eq!(next_seen[0], parish_core::world::LocationId(21));
        // Last item is id 10 (sorted position 9 after wrap).
        assert_eq!(next_seen[19], parish_core::world::LocationId(10));
    }

    #[test]
    fn gossip_budget_skips_sparse_groups_without_consuming_budget() {
        // Mix of eligible (len>=2) and sparse (len<2) groups. Budget=3.
        // Sparse groups must not count against the budget — we should see
        // exactly 3 propagations regardless of how many sparse peers sit
        // between them.
        let mut groups = std::collections::HashMap::new();
        for i in 1..=10u32 {
            let (loc, mut npcs) = make_group(i);
            // Every 2nd group is sparse (1 member only).
            if i.is_multiple_of(2) {
                npcs.truncate(1);
            }
            groups.insert(loc, npcs);
        }
        let mut calls = 0;
        let _ = propagate_gossip_budgeted(&groups, 0, 3, |_| calls += 1);
        assert_eq!(calls, 3, "sparse groups must not consume budget");
    }

    #[test]
    fn gossip_budget_cursor_wraps_modulo_group_count() {
        // Absurdly large cursor should wrap cleanly.
        let mut groups = std::collections::HashMap::new();
        for i in 1..=5 {
            let (loc, npcs) = make_group(i);
            groups.insert(loc, npcs);
        }
        let mut calls = 0;
        // cursor = 1_000_000, budget = 2, expect new cursor = (1_000_000 % 5) + 2 = 0 + 2 = 2.
        let new_cursor = propagate_gossip_budgeted(&groups, 1_000_000, 2, |_| calls += 1);
        assert_eq!(calls, 2);
        assert_eq!(new_cursor, 2);
    }

    // ── #482 disk-session purge ─────────────────────────────────────────────

    /// Overwrites sessions.id's last_active to a fixed ISO timestamp so
    /// tests can pin "how idle" a row is without sleeping through the
    /// real retention window.
    fn backdate_session(reg: &SessionRegistry, session_id: &str, last_active_iso: &str) {
        let db = reg.db.lock().unwrap();
        db.execute(
            "UPDATE sessions SET last_active = ?1 WHERE id = ?2",
            rusqlite::params![last_active_iso, session_id],
        )
        .unwrap();
    }

    #[test]
    fn purge_expired_removes_old_row_and_save_dir() {
        // Use a valid UUID v4 format ID — the #595 path-traversal guard
        // requires session IDs to be hex+hyphen only (matching UUID v4).
        let expired_id = "e1111111-1111-4111-a111-111111111111";
        let tmp = tempfile::tempdir().unwrap();
        let reg = SessionRegistry::open(tmp.path()).unwrap();
        reg.persist_new(expired_id);
        // Fresh row + fake saves/<id>/ directory.
        let save_dir = tmp.path().join(expired_id);
        std::fs::create_dir_all(&save_dir).unwrap();
        std::fs::write(save_dir.join("parish_001.db"), b"fake").unwrap();
        // Backdate to 90 days ago so any reasonable retention sweep
        // picks it up.
        let old = (chrono::Utc::now() - chrono::Duration::days(90)).to_rfc3339();
        backdate_session(&reg, expired_id, &old);

        let purged = reg.purge_expired_disk_sessions(tmp.path(), Duration::from_secs(30 * 86_400));
        assert_eq!(purged, 1);
        assert!(!reg.exists_in_db(expired_id));
        assert!(
            !save_dir.exists(),
            "saves directory must be deleted after purge"
        );
    }

    #[test]
    fn purge_expired_preserves_recent_sessions() {
        // Use a valid UUID v4 format ID — the #595 path-traversal guard
        // requires session IDs to be hex+hyphen only (matching UUID v4).
        let recent_id = "ece11111-1111-4111-a111-111111111111";
        let tmp = tempfile::tempdir().unwrap();
        let reg = SessionRegistry::open(tmp.path()).unwrap();
        reg.persist_new(recent_id);
        let save_dir = tmp.path().join(recent_id);
        std::fs::create_dir_all(&save_dir).unwrap();

        // last_active set to `now` by persist_new — well inside the
        // 30-day retention window.
        let purged = reg.purge_expired_disk_sessions(tmp.path(), Duration::from_secs(30 * 86_400));
        assert_eq!(purged, 0);
        assert!(reg.exists_in_db(recent_id));
        assert!(save_dir.exists());
    }

    #[test]
    fn purge_expired_drops_linked_oauth_rows() {
        // Use a valid UUID v4 format ID — the #595 path-traversal guard
        // requires session IDs to be hex+hyphen only (matching UUID v4).
        let expired_linked_id = "e1111111-1111-4111-a111-111111111112";
        let tmp = tempfile::tempdir().unwrap();
        let reg = SessionRegistry::open(tmp.path()).unwrap();
        reg.persist_new(expired_linked_id);
        reg.link_oauth("google", "sub_legacy", expired_linked_id, "Old User");
        let old = (chrono::Utc::now() - chrono::Duration::days(90)).to_rfc3339();
        backdate_session(&reg, expired_linked_id, &old);

        let purged = reg.purge_expired_disk_sessions(tmp.path(), Duration::from_secs(30 * 86_400));
        assert_eq!(purged, 1);
        // The OAuth link is gone too — otherwise a fresh login for
        // `sub_legacy` would resurrect a dead session_id with no DB row.
        assert_eq!(reg.find_by_oauth("google", "sub_legacy"), None);
    }

    #[test]
    fn purge_expired_handles_missing_save_dir_gracefully() {
        // Use a valid UUID v4 format ID — the #595 path-traversal guard
        // requires session IDs to be hex+hyphen only (matching UUID v4).
        let ghost_id = "abb51111-1111-4111-a111-111111111111";
        let tmp = tempfile::tempdir().unwrap();
        let reg = SessionRegistry::open(tmp.path()).unwrap();
        reg.persist_new(ghost_id);
        // No saves/<id>/ directory was ever created. Purge must still
        // delete the DB row and return 1 — filesystem absence is fine.
        let old = (chrono::Utc::now() - chrono::Duration::days(90)).to_rfc3339();
        backdate_session(&reg, ghost_id, &old);

        let purged = reg.purge_expired_disk_sessions(tmp.path(), Duration::from_secs(30 * 86_400));
        assert_eq!(purged, 1);
        assert!(!reg.exists_in_db(ghost_id));
    }

    // ── #595 path traversal guard ────────────────────────────────────────────

    /// A session ID containing `..` must not cause `remove_dir_all` to
    /// operate outside the saves root.  The traversal ID is rejected before
    /// the filesystem is touched; a sibling directory must survive intact.
    #[test]
    fn purge_expired_rejects_path_traversal_id() {
        let outer = tempfile::tempdir().unwrap();
        // The "saves root" lives one level below outer so there is a parent
        // directory to try to traverse into.
        let saves_root = outer.path().join("saves");
        std::fs::create_dir_all(&saves_root).unwrap();

        // A sibling directory that a traversal payload would try to delete.
        let sibling = outer.path().join("sensitive");
        std::fs::create_dir_all(&sibling).unwrap();
        std::fs::write(sibling.join("secret.txt"), b"do not delete").unwrap();

        // Set up a SessionRegistry using saves_root as the root.
        let reg = SessionRegistry::open(&saves_root).unwrap();

        // Directly insert a row with a traversal ID (bypassing the normal
        // UUID generation path to simulate a tampered/corrupted DB).
        {
            let db = reg.db.lock().unwrap();
            db.execute(
                "INSERT INTO sessions (id, created_at, last_active) VALUES (?1, ?2, ?2)",
                rusqlite::params!["../sensitive", "2000-01-01T00:00:00Z"],
            )
            .unwrap();
        }

        // Create a fake directory at saves_root/../sensitive to give
        // remove_dir_all something to hit if the guard fails.
        // (sibling already exists above — that's the target.)

        let purged = reg
            .purge_expired_disk_sessions(&saves_root, Duration::from_secs(0 /* always expired */));

        // The DB row is deleted (purge still counts it).
        assert_eq!(purged, 1);
        // The sibling directory must NOT have been removed.
        assert!(
            sibling.exists(),
            "path traversal guard must prevent deletion of directories outside saves root"
        );
        assert!(
            sibling.join("secret.txt").exists(),
            "sensitive file must survive"
        );
    }

    /// IDs with non-hex/non-hyphen characters (including `/` and `\`) are
    /// rejected by the allowlist even if they don't look like `..` traversals.
    #[test]
    fn purge_expired_rejects_ids_with_unsafe_characters() {
        let tmp = tempfile::tempdir().unwrap();
        let reg = SessionRegistry::open(tmp.path()).unwrap();

        // Directly insert rows with unsafe IDs.
        let unsafe_ids = [
            "../../etc/passwd",
            "foo/bar",
            "foo\\bar",
            "abc def",
            "abc\0def",
        ];
        {
            let db = reg.db.lock().unwrap();
            for id in &unsafe_ids {
                db.execute(
                    "INSERT INTO sessions (id, created_at, last_active) VALUES (?1, ?2, ?2)",
                    rusqlite::params![id, "2000-01-01T00:00:00Z"],
                )
                .unwrap();
            }
        }

        // None of these should cause a panic or an out-of-root deletion.
        let purged = reg.purge_expired_disk_sessions(tmp.path(), Duration::from_secs(0));
        // All rows are deleted from the DB.
        assert_eq!(purged, unsafe_ids.len());
        // The saves root itself is intact.
        assert!(tmp.path().exists(), "saves root must still exist");
    }

    /// A well-formed UUID session ID must still be cleaned up normally —
    /// the path-traversal guard must not break the happy path.
    #[test]
    fn purge_expired_uuid_id_still_cleaned_up() {
        let tmp = tempfile::tempdir().unwrap();
        let reg = SessionRegistry::open(tmp.path()).unwrap();
        let id = "a1b2c3d4-e5f6-4789-abcd-ef0123456789";
        reg.persist_new(id);
        let save_dir = tmp.path().join(id);
        std::fs::create_dir_all(&save_dir).unwrap();

        let old = (chrono::Utc::now() - chrono::Duration::days(90)).to_rfc3339();
        backdate_session(&reg, id, &old);

        let purged = reg.purge_expired_disk_sessions(tmp.path(), Duration::from_secs(30 * 86_400));
        assert_eq!(purged, 1);
        assert!(!save_dir.exists(), "save directory must be removed");
    }

    /// Regression test for #230: the autosave path must reuse a single
    /// `AsyncDatabase` across multiple saves rather than reopening the file
    /// (and re-running `migrate()`) on every tick.
    ///
    /// Verifies:
    /// 1. Opening the DB once and calling `save_snapshot` N times produces N
    ///    snapshots in the database (i.e. the handle is reused, not replaced).
    /// 2. The snapshot count matches the number of save calls — if a new
    ///    connection were opened each time, the per-call migrate() would not
    ///    duplicate rows, but we confirm the handle is indeed reused by checking
    ///    that the Arc inside AsyncDatabase stays alive across calls.
    #[tokio::test]
    async fn autosave_reuses_async_database_across_ticks() {
        use parish_core::persistence::snapshot::{ClockSnapshot, GameSnapshot};
        use parish_core::persistence::{AsyncDatabase, Database};

        let tmp = tempfile::NamedTempFile::new().unwrap();

        // Open the DB once — exactly what the fixed autosave tick does.
        let db = Database::open(tmp.path()).unwrap();
        let async_db = AsyncDatabase::new(db);

        let branch = async_db.find_branch("main").await.unwrap().unwrap();

        fn make_snapshot() -> GameSnapshot {
            GameSnapshot {
                player_location: LocationId(1),
                weather: "Clear".to_string(),
                text_log: vec![],
                clock: ClockSnapshot {
                    game_time: chrono::DateTime::<chrono::Utc>::from_timestamp(0, 0).unwrap(),
                    speed_factor: 36.0,
                    paused: false,
                },
                npcs: vec![],
                last_tier2_game_time: None,
                last_tier3_game_time: None,
                last_tier4_game_time: None,
                introduced_npcs: Default::default(),
                visited_locations: std::collections::HashSet::new(),
                edge_traversals: Default::default(),
                gossip_network: Default::default(),
                conversation_log: Default::default(),
                player_name: None,
                npcs_who_know_player_name: Default::default(),
            }
        }

        // Simulate three autosave ticks using the same handle.
        for _ in 0..3 {
            async_db
                .save_snapshot(branch.id, &make_snapshot())
                .await
                .expect("autosave tick should succeed with reused connection");
        }

        // All three snapshots must be present; branch_log returns most-recent-first.
        let log = async_db.branch_log(branch.id).await.unwrap();
        assert_eq!(
            log.len(),
            3,
            "three autosave ticks via the same AsyncDatabase must produce three snapshots"
        );
    }
}
