//! Trait seam for per-session persistence (#614).
//!
//! [`SessionStore`] abstracts snapshot loading/saving, branch management,
//! advisory locking, and journal append/read so that future remote or
//! managed-auth backends can drop in without touching route handlers.
//!
//! The default implementation (`DbSessionStore` in `parish-server`) wraps
//! the existing [`parish_persistence::AsyncDatabase`] and filesystem helpers.
//!
//! # Dyn safety
//!
//! All methods return `std::pin::Pin<Box<dyn Future<...> + Send + '_>>` so
//! the trait is object-safe and `Arc<dyn SessionStore>` works without the
//! `async-trait` crate.

use std::path::PathBuf;
use std::pin::Pin;

use parish_persistence::{BranchInfo, GameSnapshot, SaveFileLock, SnapshotInfo, WorldEvent};
use parish_types::ParishError;

/// Opaque snapshot row identifier returned by [`SessionStore::save_snapshot`].
pub type SnapshotId = i64;

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
/// `session_id` is a UUID v4 string (`xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx`)
/// across all implementations. The concrete server implementation resolves it
/// to a save directory path on disk; other implementations may use it as a
/// primary key in a remote store.
pub trait SessionStore: Send + Sync + 'static {
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
}
