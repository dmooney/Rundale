//! Async wrapper around [`Database`] for use with Tokio.

use std::sync::{Arc, Mutex};

use crate::journal::WorldEvent;
use crate::snapshot::GameSnapshot;
use parish_types::ParishError;

use super::Database;
use super::branches::BranchInfo;
use super::journal::{RecoveryData, SnapshotInfo};
use super::schema::lock_recovered;

/// Async wrapper around [`Database`] for use with Tokio.
///
/// All methods delegate to `tokio::task::spawn_blocking` to avoid
/// blocking the async runtime with synchronous rusqlite calls.
#[derive(Debug, Clone)]
pub struct AsyncDatabase {
    pub(super) inner: Arc<Mutex<Database>>,
}

impl AsyncDatabase {
    /// Creates a new async wrapper around a database.
    pub fn new(db: Database) -> Self {
        Self {
            inner: Arc::new(Mutex::new(db)),
        }
    }

    /// Runs a blocking database operation on a background thread.
    ///
    /// Handles `Arc::clone`, `spawn_blocking`, poison recovery, and
    /// join-error conversion so each public method is a one-liner.
    async fn run_blocking<F, T>(&self, f: F) -> Result<T, ParishError>
    where
        F: FnOnce(&Database) -> Result<T, ParishError> + Send + 'static,
        T: Send + 'static,
    {
        let db = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            let guard = lock_recovered(&db);
            f(&guard)
        })
        .await
        .map_err(|e| ParishError::Database(e.to_string()))?
    }

    /// Saves a game snapshot.
    pub async fn save_snapshot(
        &self,
        branch_id: i64,
        snapshot: &GameSnapshot,
    ) -> Result<i64, ParishError> {
        let snapshot = snapshot.clone();
        self.run_blocking(move |db| db.save_snapshot(branch_id, &snapshot))
            .await
    }

    /// Loads the most recent snapshot for a branch.
    pub async fn load_latest_snapshot(
        &self,
        branch_id: i64,
    ) -> Result<Option<(i64, GameSnapshot)>, ParishError> {
        self.run_blocking(move |db| db.load_latest_snapshot(branch_id))
            .await
    }

    /// Loads the latest snapshot and its journal tail in one read transaction.
    pub async fn load_recovery_data(
        &self,
        branch_id: i64,
    ) -> Result<Option<RecoveryData>, ParishError> {
        self.run_blocking(move |db| db.load_recovery_data(branch_id))
            .await
    }

    /// Creates a new branch.
    pub async fn create_branch(
        &self,
        name: &str,
        parent_branch_id: Option<i64>,
    ) -> Result<i64, ParishError> {
        let name = name.to_string();
        self.run_blocking(move |db| db.create_branch(&name, parent_branch_id))
            .await
    }

    /// Creates a branch and initial snapshot in one SQLite transaction.
    pub async fn create_branch_with_snapshot(
        &self,
        name: &str,
        parent_branch_id: Option<i64>,
        snapshot: &GameSnapshot,
    ) -> Result<(i64, i64), ParishError> {
        let name = name.to_string();
        let snapshot = snapshot.clone();
        self.run_blocking(move |db| {
            db.create_branch_with_snapshot(&name, parent_branch_id, &snapshot)
        })
        .await
    }

    /// Deletes a branch and its cascade-owned state.
    pub async fn delete_branch(&self, branch_id: i64) -> Result<(), ParishError> {
        self.run_blocking(move |db| db.delete_branch(branch_id))
            .await
    }

    /// Finds a branch by name.
    pub async fn find_branch(&self, name: &str) -> Result<Option<BranchInfo>, ParishError> {
        let name = name.to_string();
        self.run_blocking(move |db| db.find_branch(&name)).await
    }

    /// Lists all branches.
    pub async fn list_branches(&self) -> Result<Vec<BranchInfo>, ParishError> {
        self.run_blocking(move |db| db.list_branches()).await
    }

    /// Appends a journal event.
    pub async fn append_event(
        &self,
        branch_id: i64,
        snapshot_id: i64,
        event: &WorldEvent,
        game_time: &str,
    ) -> Result<(), ParishError> {
        let event = event.clone();
        let game_time = game_time.to_string();
        self.run_blocking(move |db| db.append_event(branch_id, snapshot_id, &event, &game_time))
            .await
    }

    /// Appends a batch to the latest snapshot in one SQLite transaction.
    pub async fn append_events_to_latest_snapshot(
        &self,
        branch_id: i64,
        events: &[(WorldEvent, String)],
    ) -> Result<Option<i64>, ParishError> {
        let events = events.to_vec();
        self.run_blocking(move |db| db.append_events_to_latest_snapshot(branch_id, &events))
            .await
    }

    /// Returns events since a snapshot.
    pub async fn events_since_snapshot(
        &self,
        branch_id: i64,
        snapshot_id: i64,
    ) -> Result<Vec<WorldEvent>, ParishError> {
        self.run_blocking(move |db| db.events_since_snapshot(branch_id, snapshot_id))
            .await
    }

    /// Returns the journal event count.
    pub async fn journal_count(
        &self,
        branch_id: i64,
        snapshot_id: i64,
    ) -> Result<usize, ParishError> {
        self.run_blocking(move |db| db.journal_count(branch_id, snapshot_id))
            .await
    }

    /// Returns snapshot history for a branch.
    pub async fn branch_log(&self, branch_id: i64) -> Result<Vec<SnapshotInfo>, ParishError> {
        self.run_blocking(move |db| db.branch_log(branch_id)).await
    }

    /// Clears journal events after a snapshot.
    pub async fn clear_journal(&self, branch_id: i64, snapshot_id: i64) -> Result<(), ParishError> {
        self.run_blocking(move |db| db.clear_journal(branch_id, snapshot_id))
            .await
    }
}
