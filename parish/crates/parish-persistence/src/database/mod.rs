//! SQLite database layer for persistence.
//!
//! Provides [`Database`] (synchronous) and [`AsyncDatabase`] (async wrapper)
//! for managing branches, snapshots, and journal events. Uses WAL mode for
//! concurrent read/write access.
//!
//! ## Submodule layout
//!
//! | Module            | Responsibility                                          |
//! |-------------------|---------------------------------------------------------|
//! | `schema`          | DDL, WAL setup, migration helpers, `lock_recovered`     |
//! | `branches`        | Branch CRUD, `BranchInfo`, row mapping                  |
//! | `journal`         | Snapshot + journal ops, `SnapshotInfo`                  |
//! | `async_adapter`   | `AsyncDatabase` — Tokio `spawn_blocking` wrapper        |

mod async_adapter;
mod branches;
mod journal;
mod schema;

pub use async_adapter::AsyncDatabase;
pub use branches::BranchInfo;
pub use journal::{RecoveryData, SnapshotInfo};

use std::path::Path;

use rusqlite::Connection;

use crate::IntoParishDbError as _;
use crate::journal::WorldEvent;
use crate::snapshot::GameSnapshot;
use parish_types::ParishError;

/// Synchronous SQLite database handle.
///
/// Manages the persistence schema (branches, snapshots, journal_events)
/// and provides CRUD operations. All methods are blocking; for async
/// contexts, use [`AsyncDatabase`].
pub struct Database {
    pub(super) conn: Connection,
}

impl Database {
    /// Opens or creates a SQLite database at the given path.
    ///
    /// Configures WAL journal mode and NORMAL synchronous mode for
    /// performance, enables foreign key enforcement, then runs migrations
    /// to ensure the schema is current.
    pub fn open(path: &Path) -> Result<Self, ParishError> {
        let conn = Connection::open(path).db_err()?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA foreign_keys=ON;",
        )
        .db_err()?;
        let db = Self { conn };
        schema::migrate(&db.conn)?;
        Ok(db)
    }

    /// Opens an in-memory database (for testing).
    pub fn open_memory() -> Result<Self, ParishError> {
        let conn = Connection::open_in_memory().db_err()?;
        // foreign_keys must be enabled per-connection, including in-memory ones
        conn.execute_batch("PRAGMA foreign_keys=ON;").db_err()?;
        let db = Self { conn };
        schema::migrate(&db.conn)?;
        Ok(db)
    }

    /// Saves a game snapshot to the given branch.
    ///
    /// Returns the snapshot row id.
    pub fn save_snapshot(
        &self,
        branch_id: i64,
        snapshot: &GameSnapshot,
    ) -> Result<i64, ParishError> {
        journal::save_snapshot(&self.conn, branch_id, snapshot)
    }

    /// Loads the most recent snapshot for a branch.
    ///
    /// Returns `None` if no snapshots exist for the branch.
    pub fn load_latest_snapshot(
        &self,
        branch_id: i64,
    ) -> Result<Option<(i64, GameSnapshot)>, ParishError> {
        journal::load_latest_snapshot(&self.conn, branch_id)
    }

    /// Loads the latest snapshot and its journal tail in one read transaction.
    pub fn load_recovery_data(&self, branch_id: i64) -> Result<Option<RecoveryData>, ParishError> {
        journal::load_recovery_data(&self.conn, branch_id)
    }

    /// Creates a new branch with the given name.
    ///
    /// Returns the new branch row id.
    pub fn create_branch(
        &self,
        name: &str,
        parent_branch_id: Option<i64>,
    ) -> Result<i64, ParishError> {
        branches::create_branch(&self.conn, name, parent_branch_id)
    }

    /// Creates a branch and its initial snapshot in one SQLite transaction.
    ///
    /// A snapshot failure rolls back the branch row as well, so retrying the
    /// same branch name cannot be blocked by an orphaned partial fork.
    pub fn create_branch_with_snapshot(
        &self,
        name: &str,
        parent_branch_id: Option<i64>,
        snapshot: &GameSnapshot,
    ) -> Result<(i64, i64), ParishError> {
        let transaction = self.conn.unchecked_transaction().db_err()?;
        let branch_id = branches::create_branch(&transaction, name, parent_branch_id)?;
        let snapshot_id = journal::save_snapshot(&transaction, branch_id, snapshot)?;
        transaction.commit().db_err()?;
        Ok((branch_id, snapshot_id))
    }

    /// Deletes a branch and its cascade-owned snapshots/journal rows.
    pub fn delete_branch(&self, branch_id: i64) -> Result<(), ParishError> {
        let changed = self
            .conn
            .execute("DELETE FROM branches WHERE id = ?1", [branch_id])
            .db_err()?;
        if changed != 1 {
            return Err(ParishError::Database(format!(
                "branch id {branch_id} does not exist"
            )));
        }
        Ok(())
    }

    /// Finds a branch by name.
    pub fn find_branch(&self, name: &str) -> Result<Option<BranchInfo>, ParishError> {
        branches::find_branch(&self.conn, name)
    }

    /// Lists all branches.
    pub fn list_branches(&self) -> Result<Vec<BranchInfo>, ParishError> {
        branches::list_branches(&self.conn)
    }

    /// Appends a journal event for the given branch and snapshot.
    ///
    /// The sequence number is computed and inserted atomically via a single
    /// INSERT … SELECT statement, preventing duplicate sequences under
    /// concurrent writes. The UNIQUE index on (branch_id, after_snapshot_id,
    /// sequence) provides a second line of defence at the database level.
    pub fn append_event(
        &self,
        branch_id: i64,
        snapshot_id: i64,
        event: &WorldEvent,
        game_time: &str,
    ) -> Result<(), ParishError> {
        journal::append_event(&self.conn, branch_id, snapshot_id, event, game_time)
    }

    /// Appends a batch to the branch's latest snapshot atomically.
    ///
    /// Returns the selected snapshot id, or `None` when the branch has no
    /// snapshot. Snapshot lookup and all inserts are one transaction.
    pub fn append_events_to_latest_snapshot(
        &self,
        branch_id: i64,
        events: &[(WorldEvent, String)],
    ) -> Result<Option<i64>, ParishError> {
        journal::append_events_to_latest_snapshot(&self.conn, branch_id, events)
    }

    /// Returns all journal events after a given snapshot for a branch.
    pub fn events_since_snapshot(
        &self,
        branch_id: i64,
        snapshot_id: i64,
    ) -> Result<Vec<WorldEvent>, ParishError> {
        journal::events_since_snapshot(&self.conn, branch_id, snapshot_id)
    }

    /// Returns the number of journal events after a given snapshot.
    pub fn journal_count(&self, branch_id: i64, snapshot_id: i64) -> Result<usize, ParishError> {
        journal::journal_count(&self.conn, branch_id, snapshot_id)
    }

    /// Returns snapshot history for a branch (most recent first).
    pub fn branch_log(&self, branch_id: i64) -> Result<Vec<SnapshotInfo>, ParishError> {
        journal::branch_log(&self.conn, branch_id)
    }

    /// Deletes journal events for a branch after a given snapshot.
    ///
    /// Used during compaction after a new snapshot is taken.
    pub fn clear_journal(&self, branch_id: i64, snapshot_id: i64) -> Result<(), ParishError> {
        journal::clear_journal(&self.conn, branch_id, snapshot_id)
    }
}

impl std::fmt::Debug for Database {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Database").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests;
