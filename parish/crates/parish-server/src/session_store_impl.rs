//! Default `SessionStore` + `IdentityStore` + `SessionRegistry` implementations.
//!
//! [`DbSessionStore`] backs all three traits with:
//! - An `AsyncDatabase` opened on demand from the session's save directory.
//! - A `SaveFileLock` advisory lock on the active `.db` file.
//! - A `rusqlite::Connection` for the `sessions.db` identity / bookkeeping tables.
//!
//! [`SqliteIdentityStore`] and [`SqliteSessionRegistry`] are thin wrappers
//! around the same `Arc<Mutex<Connection>>` that the existing `SessionRegistry`
//! struct uses, renamed and refactored to implement the core traits.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dashmap::DashMap;

use parish_core::error::ParishError;
use parish_core::identity::{IdentityStore, SessionRegistry as SessionRegistryTrait};
use parish_core::persistence::{
    AsyncDatabase, BranchInfo, Database, GameSnapshot, SaveFileLock, SnapshotInfo, WorldEvent,
};
use parish_core::session_store::{BoxFuture, SessionStore, SnapshotId};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn lock_db(mutex: &Mutex<rusqlite::Connection>) -> MutexGuard<'_, rusqlite::Connection> {
    match mutex.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

// ── DbSessionStore ────────────────────────────────────────────────────────────

/// Per-session saves directory mapped to an `AsyncDatabase` instance.
///
/// Keyed by session_id so `save_path` resolution stays inside this struct.
struct SessionDb {
    /// Path to the `.db` file (e.g. `saves/<sid>/parish_001.db`).
    db_path: PathBuf,
    /// Cached open handle — identical to the autosave-tick optimisation (#230).
    async_db: AsyncDatabase,
}

/// Default implementation of [`SessionStore`] backed by `AsyncDatabase`.
///
/// Each session's save file is looked up from `saves/<session_id>/` by
/// finding the first alphabetically-sorted `.db` file.  This mirrors the
/// same scan performed by `restore_session` in `session.rs`.
pub struct DbSessionStore {
    saves_dir: PathBuf,
    /// Cache: session_id → open database handle.
    /// None until `ensure_db` is first called for a session.
    open_dbs: DashMap<String, Arc<SessionDb>>,
}

impl DbSessionStore {
    /// Creates a new store rooted at `saves_dir`.
    pub fn new(saves_dir: PathBuf) -> Self {
        Self {
            saves_dir,
            open_dbs: DashMap::new(),
        }
    }

    /// Returns the path to the first `.db` file in `saves/<session_id>/`.
    ///
    /// Returns `None` when no `.db` file exists yet (new session).
    fn first_db_path(&self, session_id: &str) -> Option<PathBuf> {
        let session_dir = self.saves_dir.join(session_id);
        let mut files: Vec<PathBuf> = std::fs::read_dir(&session_dir)
            .ok()?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "db"))
            .collect();
        files.sort();
        files.into_iter().next()
    }

    /// Returns (or lazily opens) a cached [`AsyncDatabase`] for the given
    /// session.  If the session has no save file yet, one is created via
    /// `parish_persistence::picker::new_save_path`.
    fn ensure_db(&self, session_id: &str) -> Result<Arc<SessionDb>, ParishError> {
        // Fast path: already open.
        if let Some(entry) = self.open_dbs.get(session_id) {
            return Ok(Arc::clone(&*entry));
        }

        let db_path = match self.first_db_path(session_id) {
            Some(p) => p,
            None => {
                // New session — create save directory + first save file.
                let session_dir = self.saves_dir.join(session_id);
                std::fs::create_dir_all(&session_dir)?;
                parish_core::persistence::picker::new_save_path(&session_dir)
            }
        };

        let db = Database::open(&db_path)?;
        let session_db = Arc::new(SessionDb {
            db_path,
            async_db: AsyncDatabase::new(db),
        });
        self.open_dbs
            .insert(session_id.to_string(), Arc::clone(&session_db));
        Ok(session_db)
    }
}

impl SessionStore for DbSessionStore {
    fn load_latest_snapshot(
        &self,
        session_id: &str,
        branch_id: i64,
    ) -> BoxFuture<'_, Result<Option<(SnapshotId, GameSnapshot)>, ParishError>> {
        let session_id = session_id.to_string();
        Box::pin(async move {
            let sdb = self.ensure_db(&session_id)?;
            sdb.async_db.load_latest_snapshot(branch_id).await
        })
    }

    fn save_snapshot(
        &self,
        session_id: &str,
        branch_id: i64,
        snapshot: &GameSnapshot,
    ) -> BoxFuture<'_, Result<SnapshotId, ParishError>> {
        let session_id = session_id.to_string();
        let snapshot = snapshot.clone();
        Box::pin(async move {
            let sdb = self.ensure_db(&session_id)?;
            sdb.async_db.save_snapshot(branch_id, &snapshot).await
        })
    }

    fn list_branches(
        &self,
        session_id: &str,
    ) -> BoxFuture<'_, Result<Vec<BranchInfo>, ParishError>> {
        let session_id = session_id.to_string();
        Box::pin(async move {
            let sdb = self.ensure_db(&session_id)?;
            sdb.async_db.list_branches().await
        })
    }

    fn create_branch(
        &self,
        session_id: &str,
        name: &str,
        parent_branch_id: Option<i64>,
    ) -> BoxFuture<'_, Result<i64, ParishError>> {
        let session_id = session_id.to_string();
        let name = name.to_string();
        Box::pin(async move {
            let sdb = self.ensure_db(&session_id)?;
            sdb.async_db.create_branch(&name, parent_branch_id).await
        })
    }

    fn load_branch(
        &self,
        session_id: &str,
        name: &str,
    ) -> BoxFuture<'_, Result<Option<BranchInfo>, ParishError>> {
        let session_id = session_id.to_string();
        let name = name.to_string();
        Box::pin(async move {
            let sdb = self.ensure_db(&session_id)?;
            sdb.async_db.find_branch(&name).await
        })
    }

    fn branch_log(
        &self,
        session_id: &str,
        branch_id: i64,
    ) -> BoxFuture<'_, Result<Vec<SnapshotInfo>, ParishError>> {
        let session_id = session_id.to_string();
        Box::pin(async move {
            let sdb = self.ensure_db(&session_id)?;
            sdb.async_db.branch_log(branch_id).await
        })
    }

    fn acquire_save_lock(&self, session_id: &str) -> BoxFuture<'_, Option<SaveFileLock>> {
        let session_id = session_id.to_string();
        Box::pin(async move {
            // Ensure the DB entry exists (creates the file if needed) so
            // `save_path` returns the correct path before we try to lock it.
            let sdb = self.ensure_db(&session_id).ok()?;
            SaveFileLock::try_acquire(&sdb.db_path)
        })
    }

    fn save_path(&self, session_id: &str) -> Option<PathBuf> {
        // Fast path from cache.
        if let Some(entry) = self.open_dbs.get(session_id) {
            return Some(entry.db_path.clone());
        }
        self.first_db_path(session_id)
    }

    fn append_journal_event(
        &self,
        session_id: &str,
        branch_id: i64,
        snapshot_id: SnapshotId,
        event: &WorldEvent,
        game_time: &str,
    ) -> BoxFuture<'_, Result<(), ParishError>> {
        let session_id = session_id.to_string();
        let event = event.clone();
        let game_time = game_time.to_string();
        Box::pin(async move {
            let sdb = self.ensure_db(&session_id)?;
            sdb.async_db
                .append_event(branch_id, snapshot_id, &event, &game_time)
                .await
        })
    }

    fn read_journal(
        &self,
        session_id: &str,
        branch_id: i64,
        snapshot_id: SnapshotId,
    ) -> BoxFuture<'_, Result<Vec<WorldEvent>, ParishError>> {
        let session_id = session_id.to_string();
        Box::pin(async move {
            let sdb = self.ensure_db(&session_id)?;
            sdb.async_db
                .events_since_snapshot(branch_id, snapshot_id)
                .await
        })
    }
}

// ── SqliteIdentityStore ───────────────────────────────────────────────────────

/// Shared reference-counted SQLite connection for the identity/session DB.
///
/// Re-used by both [`SqliteIdentityStore`] and [`SqliteSessionRegistry`] so
/// they both operate on the same `sessions.db` file, preserving the existing
/// schema and zero-migration contract from #615.
pub type SharedConn = Arc<Mutex<rusqlite::Connection>>;

/// [`IdentityStore`] backed by the `oauth_accounts` table in `sessions.db`.
pub struct SqliteIdentityStore {
    conn: SharedConn,
}

impl SqliteIdentityStore {
    pub fn new(conn: SharedConn) -> Self {
        Self { conn }
    }
}

impl IdentityStore for SqliteIdentityStore {
    fn lookup_by_provider(&self, provider: &str, provider_user_id: &str) -> Option<String> {
        let db = lock_db(&self.conn);
        db.query_row(
            "SELECT session_id FROM oauth_accounts
             WHERE provider = ?1 AND provider_user_id = ?2",
            rusqlite::params![provider, provider_user_id],
            |row| row.get(0),
        )
        .ok()
    }

    fn link_provider(
        &self,
        provider: &str,
        provider_user_id: &str,
        account_id: &str,
        display_name: &str,
    ) {
        let db = lock_db(&self.conn);
        match db.execute(
            "INSERT OR REPLACE INTO oauth_accounts
             (provider, provider_user_id, session_id, display_name) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![provider, provider_user_id, account_id, display_name],
        ) {
            Ok(rows) => tracing::info!(
                provider = %provider,
                provider_user_id = %provider_user_id,
                account_id = %account_id,
                rows = rows,
                "SqliteIdentityStore: link_provider stored account"
            ),
            Err(e) => tracing::error!(
                provider = %provider,
                provider_user_id = %provider_user_id,
                account_id = %account_id,
                error = %e,
                "SqliteIdentityStore: link_provider DB write failed"
            ),
        }
    }

    fn get_account(&self, account_id: &str) -> Option<(String, String)> {
        let db = lock_db(&self.conn);
        db.query_row(
            "SELECT provider_user_id, display_name FROM oauth_accounts \
             WHERE session_id = ?1 AND provider = 'google'",
            rusqlite::params![account_id],
            |row: &rusqlite::Row<'_>| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .ok()
    }

    fn create_account(&self, account_id: &str) {
        let now = now_iso();
        let db = lock_db(&self.conn);
        if let Err(e) = db.execute(
            "INSERT OR IGNORE INTO sessions (id, created_at, last_active) VALUES (?1, ?2, ?2)",
            rusqlite::params![account_id, now],
        ) {
            tracing::warn!(account_id = %account_id, error = %e, "SqliteIdentityStore: create_account failed");
        }
    }
}

// ── SqliteSessionRegistry ─────────────────────────────────────────────────────

/// [`SessionRegistry`] backed by the `sessions` table in `sessions.db`.
///
/// The in-memory `DashMap<session_id, Arc<SessionEntry>>` is kept on the
/// concrete [`crate::session::SessionStore`] struct (the server-side God
/// struct), not here, because `SessionEntry` carries non-persistent state
/// (JoinHandles, CancellationTokens) that does not belong behind a storage
/// trait.
pub struct SqliteSessionRegistry {
    conn: SharedConn,
    /// Last-active Unix timestamps (seconds) for the in-memory eviction check.
    /// Mirrors `AtomicU64` on `SessionEntry` but at the registry level for
    /// `cleanup_stale`.
    last_active: DashMap<String, u64>,
}

impl SqliteSessionRegistry {
    pub fn new(conn: SharedConn) -> Self {
        Self {
            conn,
            last_active: DashMap::new(),
        }
    }
}

impl SessionRegistryTrait for SqliteSessionRegistry {
    fn lookup(&self, session_id: &str) -> bool {
        let db = lock_db(&self.conn);
        db.query_row("SELECT 1 FROM sessions WHERE id = ?1", [session_id], |_| {
            Ok(())
        })
        .is_ok()
    }

    fn register(&self, session_id: &str) {
        let now = now_iso();
        let db = lock_db(&self.conn);
        if let Err(e) = db.execute(
            "INSERT OR IGNORE INTO sessions (id, created_at, last_active) VALUES (?1, ?2, ?2)",
            rusqlite::params![session_id, now],
        ) {
            tracing::warn!(session_id = %session_id, error = %e, "SqliteSessionRegistry: register failed");
        }
        self.last_active.insert(session_id.to_string(), now_unix());
    }

    fn touch(&self, session_id: &str) {
        let now_str = now_iso();
        let db = lock_db(&self.conn);
        if let Err(e) = db.execute(
            "UPDATE sessions SET last_active = ?1 WHERE id = ?2",
            rusqlite::params![now_str, session_id],
        ) {
            tracing::warn!(session_id = %session_id, error = %e, "SqliteSessionRegistry: touch failed");
        }
        self.last_active.insert(session_id.to_string(), now_unix());
    }

    fn cleanup_stale(&self, max_age: Duration) {
        let cutoff = now_unix().saturating_sub(max_age.as_secs());
        self.last_active.retain(|_, ts| *ts >= cutoff);
    }

    fn evict_idle(&self, saves_root: &Path, max_age: Duration) -> usize {
        use crate::session::is_valid_session_id;

        let cutoff_secs = now_unix().saturating_sub(max_age.as_secs());
        let cutoff = match chrono::DateTime::<chrono::Utc>::from_timestamp(cutoff_secs as i64, 0) {
            Some(dt) => dt.to_rfc3339(),
            None => {
                tracing::warn!(
                    cutoff_secs = cutoff_secs,
                    "SqliteSessionRegistry::evict_idle: cutoff out of range, skipping"
                );
                return 0;
            }
        };

        // Collect expired IDs and delete their DB rows in a single transaction.
        let expired_ids: Vec<String> = {
            let db = lock_db(&self.conn);
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
                tracing::warn!(error = %e, "SqliteSessionRegistry::evict_idle: DB read failed");
                return 0;
            }
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
                    let oauth_sql =
                        format!("DELETE FROM oauth_accounts WHERE session_id IN ({placeholders})");
                    tx.execute(&oauth_sql, params.as_slice())?;
                    tx.commit()
                })();
                if let Err(e) = tx_result {
                    tracing::warn!(error = %e, "SqliteSessionRegistry::evict_idle: DB delete failed");
                    return 0;
                }
            }
            collected
        };

        if expired_ids.is_empty() {
            return 0;
        }

        // Best-effort filesystem cleanup with the same security guards as the
        // original `purge_expired_disk_sessions` (#595, #482).
        let canonical_saves_root = match saves_root.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "SqliteSessionRegistry::evict_idle: cannot canonicalize saves_root, skipping fs cleanup"
                );
                return expired_ids.len();
            }
        };

        for id in &expired_ids {
            if !is_valid_session_id(id) {
                tracing::warn!(
                    session_id = %id,
                    "SqliteSessionRegistry::evict_idle: rejected unsafe session ID, skipping fs remove"
                );
                continue;
            }

            let session_dir = saves_root.join(id);
            if !session_dir.exists() {
                continue;
            }

            let canonical_dir = match session_dir.canonicalize() {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(
                        session_id = %id,
                        error = %e,
                        "SqliteSessionRegistry::evict_idle: cannot canonicalize session dir, skipping"
                    );
                    continue;
                }
            };
            if !canonical_dir.starts_with(&canonical_saves_root) {
                tracing::warn!(
                    session_id = %id,
                    path = %canonical_dir.display(),
                    saves_root = %canonical_saves_root.display(),
                    "SqliteSessionRegistry::evict_idle: path escapes saves root, skipping"
                );
                continue;
            }

            match std::fs::remove_dir_all(&session_dir) {
                Ok(()) => tracing::info!(
                    session_id = %id,
                    path = %session_dir.display(),
                    "SqliteSessionRegistry::evict_idle: removed saves directory"
                ),
                Err(e) => tracing::warn!(
                    session_id = %id,
                    path = %session_dir.display(),
                    error = %e,
                    "SqliteSessionRegistry::evict_idle: failed to remove saves directory"
                ),
            }
        }

        expired_ids.len()
    }
}

// ── open_sessions_db ──────────────────────────────────────────────────────────

/// Opens (or creates) `saves/sessions.db`, runs schema migrations, and
/// returns a shared `Arc<Mutex<Connection>>` that [`SqliteIdentityStore`] and
/// [`SqliteSessionRegistry`] both share.
///
/// This is the single place where the identity/session schema is defined,
/// preserving the existing table layout so no migration is needed.
pub fn open_sessions_db(saves_dir: &Path) -> rusqlite::Result<SharedConn> {
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
    // Idempotent migration: add display_name to existing DBs.
    let _ = conn.execute_batch(
        "ALTER TABLE oauth_accounts ADD COLUMN display_name TEXT NOT NULL DEFAULT ''",
    );
    Ok(Arc::new(Mutex::new(conn)))
}

// ── InMemorySessionStore (test-only) ─────────────────────────────────────────

/// Minimal in-memory [`SessionStore`] for unit tests.
///
/// Stores snapshots in a `HashMap<(session_id, branch_id), Vec<GameSnapshot>>`.
/// Branches are auto-created on first use with monotonically increasing IDs.
/// Journals are stored per `(session_id, branch_id, snapshot_id)` key.
///
/// This implementation exists solely to prove the trait is genuinely backend-
/// agnostic — production code always uses [`DbSessionStore`].
#[cfg(test)]
pub(crate) struct InMemorySessionStore {
    snapshots: std::sync::Mutex<
        std::collections::HashMap<(String, i64), Vec<parish_core::persistence::GameSnapshot>>,
    >,
    branches: std::sync::Mutex<
        std::collections::HashMap<String, Vec<parish_core::persistence::BranchInfo>>,
    >,
    next_branch_id: std::sync::Mutex<i64>,
    next_snapshot_id: std::sync::Mutex<i64>,
    journal: std::sync::Mutex<
        std::collections::HashMap<(String, i64, i64), Vec<parish_core::persistence::WorldEvent>>,
    >,
}

#[cfg(test)]
impl InMemorySessionStore {
    pub(crate) fn new() -> Self {
        Self {
            snapshots: std::sync::Mutex::new(std::collections::HashMap::new()),
            branches: std::sync::Mutex::new(std::collections::HashMap::new()),
            next_branch_id: std::sync::Mutex::new(1),
            next_snapshot_id: std::sync::Mutex::new(1),
            journal: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    fn next_branch_id(&self) -> i64 {
        let mut id = self.next_branch_id.lock().unwrap();
        let v = *id;
        *id += 1;
        v
    }

    fn next_snapshot_id(&self) -> i64 {
        let mut id = self.next_snapshot_id.lock().unwrap();
        let v = *id;
        *id += 1;
        v
    }
}

#[cfg(test)]
impl SessionStore for InMemorySessionStore {
    fn load_latest_snapshot(
        &self,
        session_id: &str,
        branch_id: i64,
    ) -> BoxFuture<
        '_,
        Result<
            Option<(SnapshotId, parish_core::persistence::GameSnapshot)>,
            parish_core::error::ParishError,
        >,
    > {
        let key = (session_id.to_string(), branch_id);
        let snaps = self.snapshots.lock().unwrap();
        let result = snaps.get(&key).and_then(|v| {
            if v.is_empty() {
                None
            } else {
                let snap_id = v.len() as i64;
                Some((snap_id, v.last().unwrap().clone()))
            }
        });
        Box::pin(std::future::ready(Ok(result)))
    }

    fn save_snapshot(
        &self,
        session_id: &str,
        branch_id: i64,
        snapshot: &parish_core::persistence::GameSnapshot,
    ) -> BoxFuture<'_, Result<SnapshotId, parish_core::error::ParishError>> {
        let snap_id = self.next_snapshot_id();
        let key = (session_id.to_string(), branch_id);
        self.snapshots
            .lock()
            .unwrap()
            .entry(key)
            .or_default()
            .push(snapshot.clone());
        Box::pin(std::future::ready(Ok(snap_id)))
    }

    fn list_branches(
        &self,
        session_id: &str,
    ) -> BoxFuture<
        '_,
        Result<Vec<parish_core::persistence::BranchInfo>, parish_core::error::ParishError>,
    > {
        let branches = self.branches.lock().unwrap();
        let result = branches.get(session_id).cloned().unwrap_or_default();
        Box::pin(std::future::ready(Ok(result)))
    }

    fn create_branch(
        &self,
        session_id: &str,
        name: &str,
        parent_branch_id: Option<i64>,
    ) -> BoxFuture<'_, Result<i64, parish_core::error::ParishError>> {
        let branch_id = self.next_branch_id();
        let branch = parish_core::persistence::BranchInfo {
            id: branch_id,
            name: name.to_string(),
            parent_branch_id,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        self.branches
            .lock()
            .unwrap()
            .entry(session_id.to_string())
            .or_default()
            .push(branch);
        Box::pin(std::future::ready(Ok(branch_id)))
    }

    fn load_branch(
        &self,
        session_id: &str,
        name: &str,
    ) -> BoxFuture<
        '_,
        Result<Option<parish_core::persistence::BranchInfo>, parish_core::error::ParishError>,
    > {
        let branches = self.branches.lock().unwrap();
        let result = branches
            .get(session_id)
            .and_then(|v| v.iter().find(|b| b.name == name).cloned());
        Box::pin(std::future::ready(Ok(result)))
    }

    fn branch_log(
        &self,
        session_id: &str,
        branch_id: i64,
    ) -> BoxFuture<
        '_,
        Result<Vec<parish_core::persistence::SnapshotInfo>, parish_core::error::ParishError>,
    > {
        let snaps = self.snapshots.lock().unwrap();
        let count = snaps
            .get(&(session_id.to_string(), branch_id))
            .map(|v| v.len())
            .unwrap_or(0);
        // Return fake SnapshotInfo entries — enough for length assertions.
        let now = chrono::Utc::now().to_rfc3339();
        let result: Vec<parish_core::persistence::SnapshotInfo> = (0..count as i64)
            .rev()
            .map(|i| parish_core::persistence::SnapshotInfo {
                id: i + 1,
                game_time: now.clone(),
                real_time: now.clone(),
            })
            .collect();
        Box::pin(std::future::ready(Ok(result)))
    }

    fn acquire_save_lock(
        &self,
        _session_id: &str,
    ) -> BoxFuture<'_, Option<parish_core::persistence::SaveFileLock>> {
        // In-memory impl has no file to lock.
        Box::pin(std::future::ready(None))
    }

    fn save_path(&self, _session_id: &str) -> Option<std::path::PathBuf> {
        None
    }

    fn append_journal_event(
        &self,
        session_id: &str,
        branch_id: i64,
        snapshot_id: SnapshotId,
        event: &parish_core::persistence::WorldEvent,
        _game_time: &str,
    ) -> BoxFuture<'_, Result<(), parish_core::error::ParishError>> {
        self.journal
            .lock()
            .unwrap()
            .entry((session_id.to_string(), branch_id, snapshot_id))
            .or_default()
            .push(event.clone());
        Box::pin(std::future::ready(Ok(())))
    }

    fn read_journal(
        &self,
        session_id: &str,
        branch_id: i64,
        snapshot_id: SnapshotId,
    ) -> BoxFuture<
        '_,
        Result<Vec<parish_core::persistence::WorldEvent>, parish_core::error::ParishError>,
    > {
        let journal = self.journal.lock().unwrap();
        let events = journal
            .get(&(session_id.to_string(), branch_id, snapshot_id))
            .cloned()
            .unwrap_or_default();
        Box::pin(std::future::ready(Ok(events)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parish_core::persistence::Database;
    use parish_core::persistence::WorldEvent;

    fn make_snapshot() -> parish_core::persistence::GameSnapshot {
        use chrono::TimeZone;
        use parish_core::persistence::snapshot::{ClockSnapshot, GameSnapshot};
        use parish_core::world::LocationId;
        GameSnapshot {
            player_location: LocationId(1),
            weather: "Clear".to_string(),
            text_log: vec![],
            clock: ClockSnapshot {
                game_time: chrono::Utc.with_ymd_and_hms(1820, 3, 20, 8, 0, 0).unwrap(),
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

    // ── SqliteIdentityStore round-trip ────────────────────────────────────────

    #[test]
    fn identity_store_link_and_lookup() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = open_sessions_db(tmp.path()).unwrap();
        let store = SqliteIdentityStore::new(Arc::clone(&conn));

        store.create_account("sess_001");
        store.link_provider("google", "sub_abc", "sess_001", "Alice Test");

        assert_eq!(
            store.lookup_by_provider("google", "sub_abc"),
            Some("sess_001".to_string()),
            "lookup_by_provider must return the linked account_id"
        );
        assert_eq!(
            store.get_account("sess_001"),
            Some(("sub_abc".to_string(), "Alice Test".to_string())),
            "get_account must return (sub, display_name)"
        );
    }

    #[test]
    fn identity_store_lookup_missing_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = open_sessions_db(tmp.path()).unwrap();
        let store = SqliteIdentityStore::new(conn);
        assert_eq!(store.lookup_by_provider("google", "nobody"), None);
        assert_eq!(store.get_account("no_session"), None);
    }

    // ── SqliteSessionRegistry round-trip ──────────────────────────────────────

    #[test]
    fn session_registry_register_and_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = open_sessions_db(tmp.path()).unwrap();
        let reg = SqliteSessionRegistry::new(conn);

        assert!(!reg.lookup("sess_xyz"));
        reg.register("sess_xyz");
        assert!(reg.lookup("sess_xyz"));
    }

    #[test]
    fn session_registry_touch_updates_timestamp() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = open_sessions_db(tmp.path()).unwrap();
        let reg = SqliteSessionRegistry::new(Arc::clone(&conn));

        reg.register("sess_touch");
        // back-date the row
        {
            let db = conn.lock().unwrap();
            db.execute(
                "UPDATE sessions SET last_active = '2000-01-01T00:00:00Z' WHERE id = ?1",
                rusqlite::params!["sess_touch"],
            )
            .unwrap();
        }
        reg.touch("sess_touch");
        let last_active: String = {
            let db = conn.lock().unwrap();
            db.query_row(
                "SELECT last_active FROM sessions WHERE id = ?1",
                rusqlite::params!["sess_touch"],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert!(
            last_active.as_str() > "2000-01-01",
            "touch must update last_active beyond the backdated value"
        );
    }

    // ── DbSessionStore round-trip ─────────────────────────────────────────────

    #[tokio::test]
    async fn db_session_store_save_and_load_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let session_id = "a1b2c3d4-e5f6-4789-abcd-ef0123456789";
        // Pre-create session directory (as session.rs does).
        let session_dir = tmp.path().join(session_id);
        std::fs::create_dir_all(&session_dir).unwrap();

        // Seed a save file so DbSessionStore can find it.
        let save_path = session_dir.join("parish_001.db");
        {
            let db = Database::open(&save_path).unwrap();
            let main_branch = db.find_branch("main").unwrap().unwrap();
            db.save_snapshot(main_branch.id, &make_snapshot()).unwrap();
        }

        let store = DbSessionStore::new(tmp.path().to_path_buf());

        // List branches — should return ["main"].
        let branches = store.list_branches(session_id).await.unwrap();
        assert_eq!(branches.len(), 1);
        assert_eq!(branches[0].name, "main");

        let branch_id = branches[0].id;

        // Load latest snapshot — should succeed.
        let loaded = store
            .load_latest_snapshot(session_id, branch_id)
            .await
            .unwrap();
        assert!(
            loaded.is_some(),
            "load_latest_snapshot must return Some after seeding"
        );

        // Save a new snapshot.
        let snap_id = store
            .save_snapshot(session_id, branch_id, &make_snapshot())
            .await
            .unwrap();
        assert!(snap_id > 0);

        // Branch log should now have 2 snapshots.
        let log = store.branch_log(session_id, branch_id).await.unwrap();
        assert_eq!(log.len(), 2, "branch_log must reflect both snapshots");
    }

    #[tokio::test]
    async fn db_session_store_create_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let session_id = "b1b2c3d4-e5f6-4789-abcd-ef0123456789";
        let session_dir = tmp.path().join(session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        let save_path = session_dir.join("parish_001.db");
        Database::open(&save_path).unwrap();

        let store = DbSessionStore::new(tmp.path().to_path_buf());
        let fork_id = store.create_branch(session_id, "fork", None).await.unwrap();
        let found = store.load_branch(session_id, "fork").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, fork_id);
    }

    #[tokio::test]
    async fn db_session_store_journal_append_and_read() {
        let tmp = tempfile::tempdir().unwrap();
        let session_id = "c1b2c3d4-e5f6-4789-abcd-ef0123456789";
        let session_dir = tmp.path().join(session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        let save_path = session_dir.join("parish_001.db");
        {
            let db = Database::open(&save_path).unwrap();
            let main = db.find_branch("main").unwrap().unwrap();
            db.save_snapshot(main.id, &make_snapshot()).unwrap();
        }

        let store = DbSessionStore::new(tmp.path().to_path_buf());
        let branches = store.list_branches(session_id).await.unwrap();
        let branch_id = branches[0].id;
        let (snap_id, _) = store
            .load_latest_snapshot(session_id, branch_id)
            .await
            .unwrap()
            .unwrap();

        let event = WorldEvent::ClockAdvanced { minutes: 30 };
        store
            .append_journal_event(
                session_id,
                branch_id,
                snap_id,
                &event,
                "1820-03-20T08:00:00Z",
            )
            .await
            .unwrap();

        let events = store
            .read_journal(session_id, branch_id, snap_id)
            .await
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], event);
    }

    #[test]
    fn db_session_store_acquire_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let session_id = "d1b2c3d4-e5f6-4789-abcd-ef0123456789";
        let session_dir = tmp.path().join(session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        let save_path = session_dir.join("parish_001.db");
        Database::open(&save_path).unwrap();

        let store = DbSessionStore::new(tmp.path().to_path_buf());
        // save_path must resolve even before the async_db is opened.
        let path = store.save_path(session_id);
        assert!(
            path.is_some(),
            "save_path must return Some for an existing save file"
        );
    }

    // ── InMemorySessionStore round-trip ───────────────────────────────────────
    //
    // Exercises the `SessionStore` trait against a pure in-memory backend to
    // prove the trait is backend-agnostic and not accidentally coupled to
    // `AsyncDatabase` or the filesystem.

    #[tokio::test]
    async fn in_memory_session_store_roundtrip() {
        let store = InMemorySessionStore::new();
        let session_id = "e1b2c3d4-e5f6-4789-abcd-ef0123456789";

        // Create branch.
        let branch_id = store
            .create_branch(session_id, "main", None)
            .await
            .expect("create_branch must succeed");
        assert!(branch_id > 0);

        // Branch appears in list.
        let branches = store.list_branches(session_id).await.unwrap();
        assert_eq!(branches.len(), 1);
        assert_eq!(branches[0].name, "main");

        // Load branch by name.
        let found = store.load_branch(session_id, "main").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, branch_id);

        // No snapshot yet — load_latest returns None.
        let loaded = store
            .load_latest_snapshot(session_id, branch_id)
            .await
            .unwrap();
        assert!(loaded.is_none(), "no snapshots yet");

        // Save a snapshot.
        let snap_id = store
            .save_snapshot(session_id, branch_id, &make_snapshot())
            .await
            .unwrap();
        assert!(snap_id > 0);

        // Now load_latest returns Some.
        let (loaded_id, _snap) = store
            .load_latest_snapshot(session_id, branch_id)
            .await
            .unwrap()
            .expect("snapshot must be present after saving");
        assert!(loaded_id > 0);

        // Branch log has one entry.
        let log = store.branch_log(session_id, branch_id).await.unwrap();
        assert_eq!(log.len(), 1, "branch_log must reflect the saved snapshot");

        // Journal append and read.
        let event = WorldEvent::ClockAdvanced { minutes: 15 };
        store
            .append_journal_event(
                session_id,
                branch_id,
                snap_id,
                &event,
                "1820-03-20T09:00:00Z",
            )
            .await
            .unwrap();

        let events = store
            .read_journal(session_id, branch_id, snap_id)
            .await
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], event);

        // acquire_save_lock returns None for in-memory (no file to lock) — no panic.
        let lock = store.acquire_save_lock(session_id).await;
        assert!(lock.is_none(), "in-memory store has no file lock");

        // save_path returns None.
        assert!(store.save_path(session_id).is_none());
    }

    #[tokio::test]
    async fn in_memory_session_store_multiple_snapshots() {
        // Verify branch_log grows with each save.
        let store = InMemorySessionStore::new();
        let session_id = "f1b2c3d4-e5f6-4789-abcd-ef0123456789";
        let branch_id = store.create_branch(session_id, "main", None).await.unwrap();

        for _ in 0..3 {
            store
                .save_snapshot(session_id, branch_id, &make_snapshot())
                .await
                .unwrap();
        }

        let log = store.branch_log(session_id, branch_id).await.unwrap();
        assert_eq!(
            log.len(),
            3,
            "three saves must produce three branch_log entries"
        );
    }
}
