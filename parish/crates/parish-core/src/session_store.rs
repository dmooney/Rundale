//! Trait seam for per-session persistence (#614).
//!
//! [`SessionStore`] abstracts snapshot loading/saving, branch management,
//! advisory locking, and journal append/read so that future remote or
//! managed-auth backends can drop in without touching route handlers.
//!
//! The default implementation [`DbSessionStore`] is defined in this module
//! and wraps the existing [`parish_persistence::AsyncDatabase`] and
//! filesystem helpers.  It is available to all three runtimes (server, Tauri,
//! CLI) via `parish_core::session_store::DbSessionStore`.
//!
//! `SqliteIdentityStore` and `SqliteSessionRegistry` remain in
//! `parish-server` because they use server-only types (`is_valid_session_id`,
//! rusqlite direct access for the sessions/oauth tables).
//!
//! # Dyn safety
//!
//! All methods return `std::pin::Pin<Box<dyn Future<...> + Send + '_>>` so
//! the trait is object-safe and `Arc<dyn SessionStore>` works without the
//! `async-trait` crate.
//!
//! # Session ID convention — single-user runtimes
//!
//! Tauri and headless CLI are single-user: they use `""` as the session ID.
//! `DbSessionStore::ensure_db("")` resolves to `saves_dir` directly (joining
//! an empty string is a no-op on all platforms) and scans for `.db` files
//! there, matching the flat `saves/parish_NNN.db` layout used by both runtimes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, RwLock};

use crate::error::ParishError;
use crate::persistence::{
    AsyncDatabase, BranchInfo, Database, GameSnapshot, SaveFileLock, SnapshotInfo, WorldEvent,
    replay_journal,
};
use crate::world::WorldState;
pub use parish_types::{PlayerProgress, PlayerTask};

/// Opaque snapshot row identifier returned by [`SessionStore::save_snapshot`].
pub type SnapshotId = i64;

/// Validated, already-opened save binding whose final publication cannot fail.
///
/// Lifecycle operations prepare this token before writing the active-identity
/// commit record, then consume it immediately after the marker is durable.
pub struct PreparedSaveBinding<'a> {
    commit: Option<Box<dyn FnOnce() + Send + 'a>>,
}

impl<'a> PreparedSaveBinding<'a> {
    /// Creates a prepared binding from an infallible publication closure.
    pub fn new(commit: impl FnOnce() + Send + 'a) -> Self {
        Self {
            commit: Some(Box::new(commit)),
        }
    }

    /// Publishes the prepared binding.
    pub fn commit(mut self) {
        self.commit
            .take()
            .expect("prepared save binding may only be committed once")();
    }
}

/// A boxed, heap-allocated async future.
///
/// Used as the return type for every `SessionStore` method so the trait is
/// dyn-compatible (`Arc<dyn SessionStore>` works without `async-trait`).
pub type BoxFuture<'a, T> = Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

/// Abstract per-session persistence.
///
/// Implementors store and retrieve [`GameSnapshot`]s, manage named branches,
/// hold an advisory file lock over the active save, and record/replay a
/// [`WorldEvent`] journal.
///
/// The default server implementation (`DbSessionStore`) wraps the existing
/// `AsyncDatabase` + `SaveFileLock` machinery in `parish-persistence`.
/// A future cloud-backed or in-memory implementation need only implement
/// this trait to slot in without touching any route handler.
///
/// # Session ID convention
///
/// Multi-user runtimes (server) use a UUID v4 string
/// (`xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx`) so each session maps to its own
/// subdirectory under `saves_dir`.
///
/// Single-user runtimes (Tauri, CLI) pass `""` because `saves_dir.join("")`
/// resolves to `saves_dir` itself, matching the flat `saves/parish_NNN.db`
/// layout used by both. See [`DbSessionStore`] for details.
pub trait SessionStore: Send + Sync + 'static {
    /// Selects the exact save file backing subsequent operations for a session.
    ///
    /// The path must be a direct `.db` child of this session's configured save
    /// directory. Calling this again intentionally rebinds the session, which
    /// is required when a player switches save files.
    fn prepare_active_save<'a>(
        &'a self,
        session_id: &str,
        save_path: &Path,
    ) -> Result<PreparedSaveBinding<'a>, ParishError>;

    /// Validates, opens, and immediately publishes an active save binding.
    fn set_active_save(&self, session_id: &str, save_path: &Path) -> Result<(), ParishError> {
        self.prepare_active_save(session_id, save_path)?.commit();
        Ok(())
    }

    // ── Snapshots ─────────────────────────────────────────────────────────────

    /// Loads the most recent snapshot for the given branch.
    ///
    /// Returns `None` if no snapshots exist for `branch_id`.
    fn load_latest_snapshot(
        &self,
        session_id: &str,
        branch_id: i64,
    ) -> BoxFuture<'_, Result<Option<(SnapshotId, GameSnapshot)>, ParishError>>;

    /// Persists a snapshot for the given branch.
    ///
    /// Returns the new snapshot's row identifier.
    fn save_snapshot(
        &self,
        session_id: &str,
        branch_id: i64,
        snapshot: &GameSnapshot,
    ) -> BoxFuture<'_, Result<SnapshotId, ParishError>>;

    // ── Branches ──────────────────────────────────────────────────────────────

    /// Returns all branches for the given session, ordered by creation time.
    fn list_branches(
        &self,
        session_id: &str,
    ) -> BoxFuture<'_, Result<Vec<BranchInfo>, ParishError>>;

    /// Creates a new named branch.
    ///
    /// Returns the new branch's row identifier.
    fn create_branch(
        &self,
        session_id: &str,
        name: &str,
        parent_branch_id: Option<i64>,
    ) -> BoxFuture<'_, Result<i64, ParishError>>;

    /// Creates a branch and its initial snapshot atomically.
    ///
    /// Implementations must not leave a branch row behind when snapshot
    /// persistence fails.
    fn create_branch_with_snapshot(
        &self,
        session_id: &str,
        name: &str,
        parent_branch_id: Option<i64>,
        snapshot: &GameSnapshot,
    ) -> BoxFuture<'_, Result<(i64, SnapshotId), ParishError>>;

    /// Looks up a branch by name.
    fn load_branch(
        &self,
        session_id: &str,
        name: &str,
    ) -> BoxFuture<'_, Result<Option<BranchInfo>, ParishError>>;

    /// Returns the snapshot history for a branch (most-recent first).
    fn branch_log(
        &self,
        session_id: &str,
        branch_id: i64,
    ) -> BoxFuture<'_, Result<Vec<SnapshotInfo>, ParishError>>;

    // ── Advisory locking ──────────────────────────────────────────────────────

    /// Attempts to acquire an advisory write lock for the session's save file.
    ///
    /// Returns `Some(lock)` on success, or `None` if another live process
    /// already holds the lock (or the lock cannot be obtained).
    ///
    /// The concrete filesystem-backed implementation delegates to
    /// [`SaveFileLock::try_acquire`].
    fn acquire_save_lock(&self, session_id: &str) -> BoxFuture<'_, Option<SaveFileLock>>;

    /// Releases the advisory write lock previously acquired by [`acquire_save_lock`].
    ///
    /// Callers pass back the [`SaveFileLock`] they received from
    /// `acquire_save_lock`; dropping it is sufficient to release the lock
    /// (RAII).  This method exists as an explicit trait seam so that
    /// non-filesystem implementations (e.g. a distributed lock service) can
    /// override the release behaviour without the caller needing to know the
    /// concrete type.
    ///
    /// The default (filesystem) implementation is a no-op: the lock is
    /// released when `lock` is dropped at the end of this call.
    fn release_save_lock(&self, _session_id: &str, lock: SaveFileLock) {
        drop(lock); // RAII release — the lock file is unlocked when dropped.
    }

    /// Returns the filesystem path of the active save file for a session.
    ///
    /// Route handlers need this to populate `AppState.save_path` so the
    /// autosave tick and explicit `/save` routes know where to write.
    fn save_path(&self, session_id: &str) -> Option<PathBuf>;

    // ── Journal ───────────────────────────────────────────────────────────────

    /// Appends a [`WorldEvent`] to the journal for the given branch and snapshot.
    fn append_journal_event(
        &self,
        session_id: &str,
        branch_id: i64,
        snapshot_id: SnapshotId,
        event: &WorldEvent,
        game_time: &str,
    ) -> BoxFuture<'_, Result<(), ParishError>>;

    /// Returns all journal events recorded after `snapshot_id` for `branch_id`.
    fn read_journal(
        &self,
        session_id: &str,
        branch_id: i64,
        snapshot_id: SnapshotId,
    ) -> BoxFuture<'_, Result<Vec<WorldEvent>, ParishError>>;

    /// Loads one snapshot and its journal tail while pinned to an exact save.
    ///
    /// Implementations with mutable active-save routing should override this
    /// method so a concurrent rebind cannot interpose between the snapshot and
    /// journal reads. The default preserves compatibility for remote/test
    /// stores whose session key already identifies an immutable backing store.
    fn load_recovery_bundle_exact<'a>(
        &'a self,
        session_id: &'a str,
        save_path: &'a Path,
        branch_id: i64,
    ) -> BoxFuture<'a, Result<Option<RecoveryBundle>, ParishError>> {
        Box::pin(async move {
            self.set_active_save(session_id, save_path)?;
            let Some((snapshot_id, snapshot)) =
                self.load_latest_snapshot(session_id, branch_id).await?
            else {
                return Ok(None);
            };
            let journal = self
                .read_journal(session_id, branch_id, snapshot_id)
                .await?;
            Ok(Some(RecoveryBundle {
                snapshot_id,
                snapshot,
                journal,
            }))
        })
    }

    /// Appends task post-states while pinned to one exact save and snapshot.
    ///
    /// Implementations must commit the entire batch atomically: returning an
    /// error after persisting only a prefix would make an otherwise rolled-back
    /// staged turn reappear partially during crash recovery.
    fn append_task_mutations_exact<'a>(
        &'a self,
        target: &'a TaskJournalTarget,
        tasks: &'a [PlayerTask],
    ) -> BoxFuture<'a, Result<usize, ParishError>>;
}

/// Exact persistence identity captured for one task-producing operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskJournalTarget {
    /// Runtime session identifier (UUID on the server, empty for single-user).
    pub session_id: String,
    /// Exact active SQLite save file.
    pub save_path: PathBuf,
    /// Active branch within `save_path`.
    pub branch_id: i64,
}

/// Snapshot plus only the journal tail belonging to that exact snapshot.
#[derive(Debug, Clone)]
pub struct RecoveryBundle {
    /// Snapshot row the journal tail is anchored to.
    pub snapshot_id: SnapshotId,
    /// Canonical saved world state.
    pub snapshot: GameSnapshot,
    /// Ordered mutations recorded after `snapshot_id`.
    pub journal: Vec<WorldEvent>,
}

impl RecoveryBundle {
    /// Restores the snapshot and then replays its exact journal tail.
    pub fn restore(
        self,
        world: &mut WorldState,
        npc_manager: &mut crate::npc::manager::NpcManager,
    ) {
        self.snapshot.restore(world, npc_manager);
        replay_journal(world, npc_manager, &self.journal);
    }
}

/// Loads a crash-recovery bundle from one exact save file and branch.
pub async fn load_recovery_bundle(
    store: &dyn SessionStore,
    session_id: &str,
    save_path: &Path,
    branch_id: i64,
) -> Result<Option<RecoveryBundle>, ParishError> {
    store
        .load_recovery_bundle_exact(session_id, save_path, branch_id)
        .await
}

/// Durably appends canonical player-task post-states before a turn is acknowledged.
///
/// Each task record is a complete, idempotent state replacement. Production
/// stores select the latest snapshot and append the whole batch atomically;
/// runtimes also serialize snapshot capture/commit with task mutation/append.
pub async fn append_task_mutations(
    store: &dyn SessionStore,
    target: &TaskJournalTarget,
    tasks: &[PlayerTask],
) -> Result<usize, ParishError> {
    store.append_task_mutations_exact(target, tasks).await
}

/// Appends task mutations or restores the caller's pre-turn task ledger.
///
/// The caller must hold its per-session persistence barrier continuously from
/// before `before` and `target` were captured through this call. Keeping the
/// rollback inside the same barrier makes a failed turn retryable: the same
/// player action can produce the mutation again instead of seeing a partially
/// advanced in-memory task.
pub async fn append_task_mutations_or_rollback(
    store: &dyn SessionStore,
    target: Option<&TaskJournalTarget>,
    tasks: &[PlayerTask],
    world: &tokio::sync::Mutex<WorldState>,
    before: PlayerProgress,
) -> Result<usize, ParishError> {
    if tasks.is_empty() {
        return Ok(0);
    }
    let result = match target {
        Some(target) => append_task_mutations(store, target, tasks).await,
        None => Err(ParishError::Database(
            "cannot journal player task without an active save and branch".to_string(),
        )),
    };
    if let Err(error) = result {
        world.lock().await.player_progress = before;
        return Err(error);
    }
    result
}

fn task_event_game_time(task: &PlayerTask) -> String {
    task.completed_at
        .or(task.started_at)
        .unwrap_or(task.assigned_at)
        .to_rfc3339()
}

fn missing_task_snapshot_error(target: &TaskJournalTarget) -> ParishError {
    ParishError::Database(format!(
        "cannot journal player task: branch {} has no snapshot in {}",
        target.branch_id,
        target.save_path.display()
    ))
}

// ── DbSessionStore ────────────────────────────────────────────────────────────

/// Per-session saves directory mapped to an `AsyncDatabase` instance.
struct SessionDb {
    /// Path to the `.db` file (e.g. `saves/<sid>/parish_001.db`).
    db_path: std::path::PathBuf,
    /// Cached open handle.
    async_db: AsyncDatabase,
}

/// Default implementation of [`SessionStore`] backed by `AsyncDatabase`.
///
/// Each session's save file is looked up by finding the first
/// alphabetically-sorted `.db` file in `saves_dir/<session_id>/`.
///
/// # Single-user runtimes (Tauri, CLI)
///
/// Pass `session_id = ""` — `saves_dir.join("")` is `saves_dir` itself, so
/// the store scans the flat `saves/parish_NNN.db` layout directly.
///
/// # Multi-user runtime (server)
///
/// Pass the UUID session cookie — `saves_dir.join(session_id)` gives the
/// per-session subdirectory.
pub struct DbSessionStore {
    saves_dir: std::path::PathBuf,
    /// Cache: session_id → open database handle.
    open_dbs: RwLock<HashMap<String, Arc<SessionDb>>>,
}

impl DbSessionStore {
    /// Creates a new store rooted at `saves_dir`.
    pub fn new(saves_dir: std::path::PathBuf) -> Self {
        Self {
            saves_dir,
            open_dbs: RwLock::new(HashMap::new()),
        }
    }

    fn validated_save_path(
        &self,
        session_id: &str,
        save_path: &Path,
    ) -> Result<PathBuf, ParishError> {
        if save_path
            .extension()
            .is_none_or(|extension| extension != "db")
        {
            return Err(ParishError::Config(format!(
                "active save must be a .db file: {}",
                save_path.display()
            )));
        }

        let expected_dir = self.saves_dir.join(session_id);
        std::fs::create_dir_all(&expected_dir)?;
        let expected_dir = std::fs::canonicalize(expected_dir)?;
        let canonical_path = std::fs::canonicalize(save_path)?;
        if canonical_path.parent() != Some(expected_dir.as_path()) {
            return Err(ParishError::Config(format!(
                "active save {} is outside session directory {}",
                canonical_path.display(),
                expected_dir.display()
            )));
        }
        Ok(canonical_path)
    }

    /// Opens `save_path` without changing the active-session binding.
    ///
    /// Candidate load paths use this until snapshot deserialization and
    /// journal recovery have succeeded; a corrupt candidate must not redirect
    /// later operations away from the still-live save.
    fn open_exact_db(
        &self,
        session_id: &str,
        save_path: &Path,
    ) -> Result<Arc<SessionDb>, ParishError> {
        let db_path = self.validated_save_path(session_id, save_path)?;
        {
            let guard = self.open_dbs.read().unwrap_or_else(|p| p.into_inner());
            if let Some(entry) = guard
                .get(session_id)
                .filter(|entry| entry.db_path == db_path)
            {
                return Ok(Arc::clone(entry));
            }
        }

        let db = Database::open(&db_path)?;
        let session_db = Arc::new(SessionDb {
            db_path,
            async_db: AsyncDatabase::new(db),
        });
        Ok(session_db)
    }

    /// Opens and selects `save_path`, returning the exact selected handle.
    ///
    /// Callers retain the returned `Arc` across awaits so a concurrent save
    /// switch cannot redirect an in-flight operation.
    fn bind_exact_db(
        &self,
        session_id: &str,
        save_path: &Path,
    ) -> Result<Arc<SessionDb>, ParishError> {
        let session_db = self.open_exact_db(session_id, save_path)?;
        let mut guard = self.open_dbs.write().unwrap_or_else(|p| p.into_inner());
        guard.insert(session_id.to_string(), Arc::clone(&session_db));
        Ok(session_db)
    }

    /// Returns the path to the first `.db` file in `saves_dir/<session_id>/`.
    ///
    /// Returns `None` when no `.db` file exists yet (new session).
    fn first_db_path(&self, session_id: &str) -> Option<std::path::PathBuf> {
        let session_dir = self.saves_dir.join(session_id);
        let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&session_dir)
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
    /// `picker::new_save_path`.
    fn ensure_db(&self, session_id: &str) -> Result<Arc<SessionDb>, ParishError> {
        // Fast path: already open.
        {
            let guard = self.open_dbs.read().unwrap_or_else(|p| p.into_inner());
            if let Some(entry) = guard.get(session_id) {
                return Ok(Arc::clone(entry));
            }
        }

        let db_path = match self.first_db_path(session_id) {
            Some(p) => p,
            None => {
                // New session — create save directory + first save file.
                let session_dir = self.saves_dir.join(session_id);
                std::fs::create_dir_all(&session_dir)?;
                crate::persistence::picker::new_save_path(&session_dir)
            }
        };

        let db = Database::open(&db_path)?;
        let session_db = Arc::new(SessionDb {
            db_path,
            async_db: AsyncDatabase::new(db),
        });
        let mut guard = self.open_dbs.write().unwrap_or_else(|p| p.into_inner());
        guard.insert(session_id.to_string(), Arc::clone(&session_db));
        Ok(session_db)
    }
}

impl SessionStore for DbSessionStore {
    fn prepare_active_save<'a>(
        &'a self,
        session_id: &str,
        save_path: &Path,
    ) -> Result<PreparedSaveBinding<'a>, ParishError> {
        let session_db = self.open_exact_db(session_id, save_path)?;
        let session_id = session_id.to_string();
        Ok(PreparedSaveBinding::new(move || {
            let mut guard = self.open_dbs.write().unwrap_or_else(|p| p.into_inner());
            guard.insert(session_id, session_db);
        }))
    }

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

    fn create_branch_with_snapshot(
        &self,
        session_id: &str,
        name: &str,
        parent_branch_id: Option<i64>,
        snapshot: &GameSnapshot,
    ) -> BoxFuture<'_, Result<(i64, SnapshotId), ParishError>> {
        let result = self.ensure_db(session_id);
        let name = name.to_string();
        let snapshot = snapshot.clone();
        Box::pin(async move {
            let sdb = result?;
            sdb.async_db
                .create_branch_with_snapshot(&name, parent_branch_id, &snapshot)
                .await
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
            let sdb = self.ensure_db(&session_id).ok()?;
            SaveFileLock::try_acquire(&sdb.db_path)
        })
    }

    fn save_path(&self, session_id: &str) -> Option<PathBuf> {
        // Fast path from cache.
        {
            let guard = self.open_dbs.read().unwrap_or_else(|p| p.into_inner());
            if let Some(entry) = guard.get(session_id) {
                return Some(entry.db_path.clone());
            }
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

    fn load_recovery_bundle_exact<'a>(
        &'a self,
        session_id: &'a str,
        save_path: &'a Path,
        branch_id: i64,
    ) -> BoxFuture<'a, Result<Option<RecoveryBundle>, ParishError>> {
        let exact_db = self.open_exact_db(session_id, save_path);
        Box::pin(async move {
            let sdb = exact_db?;
            let Some(recovery) = sdb.async_db.load_recovery_data(branch_id).await? else {
                return Ok(None);
            };
            Ok(Some(RecoveryBundle {
                snapshot_id: recovery.snapshot_id,
                snapshot: recovery.snapshot,
                journal: recovery.journal,
            }))
        })
    }

    fn append_task_mutations_exact<'a>(
        &'a self,
        target: &'a TaskJournalTarget,
        tasks: &'a [PlayerTask],
    ) -> BoxFuture<'a, Result<usize, ParishError>> {
        if tasks.is_empty() {
            return Box::pin(async { Ok(0) });
        }
        let exact_db = self.bind_exact_db(&target.session_id, &target.save_path);
        Box::pin(async move {
            let sdb = exact_db?;
            let events = tasks
                .iter()
                .map(|task| {
                    (
                        WorldEvent::PlayerTaskStateChanged { task: task.clone() },
                        task_event_game_time(task),
                    )
                })
                .collect::<Vec<_>>();
            let Some(_snapshot_id) = sdb
                .async_db
                .append_events_to_latest_snapshot(target.branch_id, &events)
                .await?
            else {
                return Err(missing_task_snapshot_error(target));
            };
            Ok(tasks.len())
        })
    }
}
