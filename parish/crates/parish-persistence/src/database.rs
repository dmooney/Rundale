//! SQLite database layer for persistence.
//!
//! Provides [`Database`] (synchronous) and [`AsyncDatabase`] (async wrapper)
//! for managing branches, snapshots, and journal events. Uses WAL mode for
//! concurrent read/write access.

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use rusqlite::{Connection, OptionalExtension, params};

use crate::journal::WorldEvent;
use crate::snapshot::GameSnapshot;
use parish_types::ParishError;

/// Acquires a lock on `mutex`, recovering transparently from poisoning.
///
/// If a previous thread panicked while holding the database lock,
/// `Mutex::lock()` will return a [`PoisonError`]. Without recovery, every
/// subsequent call would cascade a single failure into a total application
/// crash (issue #82). SQLite writes are transactional, so the connection
/// itself remains in a consistent state after a panic; we simply log a
/// warning and return the underlying guard so database access continues
/// to work.
fn lock_recovered<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            tracing::warn!("database lock was poisoned; recovering");
            poisoned.into_inner()
        }
    }
}

/// Information about a save branch.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BranchInfo {
    /// Database row id.
    pub id: i64,
    /// Human-readable branch name (unique).
    pub name: String,
    /// When the branch was created (ISO 8601).
    pub created_at: String,
    /// Parent branch id, if forked.
    pub parent_branch_id: Option<i64>,
}

/// Information about a snapshot.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SnapshotInfo {
    /// Database row id.
    pub id: i64,
    /// Game time at snapshot (ISO 8601).
    pub game_time: String,
    /// Real wall-clock time at snapshot (ISO 8601).
    pub real_time: String,
}

/// Synchronous SQLite database handle.
///
/// Manages the persistence schema (branches, snapshots, journal_events)
/// and provides CRUD operations. All methods are blocking; for async
/// contexts, use [`AsyncDatabase`].
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Opens or creates a SQLite database at the given path.
    ///
    /// Configures WAL journal mode and NORMAL synchronous mode for
    /// performance, enables foreign key enforcement, then runs migrations
    /// to ensure the schema is current.
    pub fn open(path: &Path) -> Result<Self, ParishError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA foreign_keys=ON;",
        )?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    /// Opens an in-memory database (for testing).
    pub fn open_memory() -> Result<Self, ParishError> {
        let conn = Connection::open_in_memory()?;
        // foreign_keys must be enabled per-connection, including in-memory ones
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    /// Creates tables if they don't exist and ensures the "main" branch exists.
    fn migrate(&self) -> Result<(), ParishError> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS branches (
                id INTEGER PRIMARY KEY,
                name TEXT UNIQUE NOT NULL,
                created_at TEXT NOT NULL,
                parent_branch_id INTEGER
            );

            CREATE TABLE IF NOT EXISTS snapshots (
                id INTEGER PRIMARY KEY,
                branch_id INTEGER NOT NULL REFERENCES branches(id),
                game_time TEXT NOT NULL,
                real_time TEXT NOT NULL,
                world_state TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS journal_events (
                id INTEGER PRIMARY KEY,
                branch_id INTEGER NOT NULL,
                sequence INTEGER NOT NULL,
                after_snapshot_id INTEGER NOT NULL REFERENCES snapshots(id),
                event_type TEXT NOT NULL,
                event_data TEXT NOT NULL,
                game_time TEXT NOT NULL
            );

            DROP INDEX IF EXISTS idx_journal_branch_snap_seq;
            CREATE UNIQUE INDEX idx_journal_branch_snap_seq
                ON journal_events(branch_id, after_snapshot_id, sequence);",
        )?;

        // Ensure the "main" branch exists
        let exists: bool = self.conn.query_row(
            "SELECT COUNT(*) > 0 FROM branches WHERE name = 'main'",
            [],
            |row| row.get(0),
        )?;
        if !exists {
            self.conn.execute(
                "INSERT INTO branches (name, created_at, parent_branch_id) VALUES (?1, ?2, NULL)",
                params!["main", chrono::Utc::now().to_rfc3339()],
            )?;
        }

        Ok(())
    }

    /// Saves a game snapshot to the given branch.
    ///
    /// Returns the snapshot row id.
    pub fn save_snapshot(
        &self,
        branch_id: i64,
        snapshot: &GameSnapshot,
    ) -> Result<i64, ParishError> {
        let world_state = serde_json::to_string(snapshot)?;
        let game_time = snapshot.clock.game_time.to_rfc3339();
        let real_time = chrono::Utc::now().to_rfc3339();

        self.conn.execute(
            "INSERT INTO snapshots (branch_id, game_time, real_time, world_state)
             VALUES (?1, ?2, ?3, ?4)",
            params![branch_id, game_time, real_time, world_state],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Loads the most recent snapshot for a branch.
    ///
    /// Returns `None` if no snapshots exist for the branch.
    pub fn load_latest_snapshot(
        &self,
        branch_id: i64,
    ) -> Result<Option<(i64, GameSnapshot)>, ParishError> {
        let result: Option<(i64, String)> = self
            .conn
            .query_row(
                "SELECT id, world_state FROM snapshots
                 WHERE branch_id = ?1
                 ORDER BY id DESC LIMIT 1",
                params![branch_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        match result {
            Some((id, json)) => {
                let snapshot: GameSnapshot = serde_json::from_str(&json)?;
                Ok(Some((id, snapshot)))
            }
            None => Ok(None),
        }
    }

    /// Creates a new branch with the given name.
    ///
    /// Returns the new branch row id.
    pub fn create_branch(
        &self,
        name: &str,
        parent_branch_id: Option<i64>,
    ) -> Result<i64, ParishError> {
        let created_at = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO branches (name, created_at, parent_branch_id) VALUES (?1, ?2, ?3)",
            params![name, created_at, parent_branch_id],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Finds a branch by name.
    pub fn find_branch(&self, name: &str) -> Result<Option<BranchInfo>, ParishError> {
        self.conn
            .query_row(
                "SELECT id, name, created_at, parent_branch_id FROM branches WHERE name = ?1",
                params![name],
                |row| {
                    Ok(BranchInfo {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        created_at: row.get(2)?,
                        parent_branch_id: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(ParishError::from)
    }

    /// Lists all branches.
    pub fn list_branches(&self) -> Result<Vec<BranchInfo>, ParishError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, created_at, parent_branch_id FROM branches ORDER BY id")?;
        let rows = stmt.query_map([], |row| {
            Ok(BranchInfo {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
                parent_branch_id: row.get(3)?,
            })
        })?;
        let mut branches = Vec::new();
        for row in rows {
            branches.push(row?);
        }
        Ok(branches)
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
        let event_data = serde_json::to_string(event)?;
        let event_type = event.event_type();

        // Single atomic statement: the subquery computes COALESCE(MAX(sequence),0)+1
        // over existing rows for this (branch, snapshot). Even with an empty result
        // set the aggregate returns exactly one row, so the first event gets
        // sequence=1 correctly.
        self.conn.execute(
            "INSERT INTO journal_events
             (branch_id, sequence, after_snapshot_id, event_type, event_data, game_time)
             SELECT ?1, COALESCE(MAX(sequence), 0) + 1, ?2, ?3, ?4, ?5
             FROM journal_events
             WHERE branch_id = ?1 AND after_snapshot_id = ?2",
            params![branch_id, snapshot_id, event_type, event_data, game_time],
        )?;
        Ok(())
    }

    /// Returns all journal events after a given snapshot for a branch.
    pub fn events_since_snapshot(
        &self,
        branch_id: i64,
        snapshot_id: i64,
    ) -> Result<Vec<WorldEvent>, ParishError> {
        let mut stmt = self.conn.prepare(
            "SELECT event_data FROM journal_events
             WHERE branch_id = ?1 AND after_snapshot_id = ?2
             ORDER BY sequence ASC",
        )?;
        let rows = stmt.query_map(params![branch_id, snapshot_id], |row| {
            let data: String = row.get(0)?;
            Ok(data)
        })?;
        let mut events = Vec::new();
        for row in rows {
            let json = row?;
            let event: WorldEvent = serde_json::from_str(&json)?;
            events.push(event);
        }
        Ok(events)
    }

    /// Returns the number of journal events after a given snapshot.
    pub fn journal_count(&self, branch_id: i64, snapshot_id: i64) -> Result<usize, ParishError> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM journal_events
             WHERE branch_id = ?1 AND after_snapshot_id = ?2",
            params![branch_id, snapshot_id],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    /// Returns snapshot history for a branch (most recent first).
    pub fn branch_log(&self, branch_id: i64) -> Result<Vec<SnapshotInfo>, ParishError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, game_time, real_time FROM snapshots
             WHERE branch_id = ?1
             ORDER BY id DESC",
        )?;
        let rows = stmt.query_map(params![branch_id], |row| {
            Ok(SnapshotInfo {
                id: row.get(0)?,
                game_time: row.get(1)?,
                real_time: row.get(2)?,
            })
        })?;
        let mut infos = Vec::new();
        for row in rows {
            infos.push(row?);
        }
        Ok(infos)
    }

    /// Deletes journal events for a branch after a given snapshot.
    ///
    /// Used during compaction after a new snapshot is taken.
    pub fn clear_journal(&self, branch_id: i64, snapshot_id: i64) -> Result<(), ParishError> {
        self.conn.execute(
            "DELETE FROM journal_events
             WHERE branch_id = ?1 AND after_snapshot_id = ?2",
            params![branch_id, snapshot_id],
        )?;
        Ok(())
    }
}

impl std::fmt::Debug for Database {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Database").finish_non_exhaustive()
    }
}

/// Async wrapper around [`Database`] for use with Tokio.
///
/// All methods delegate to `tokio::task::spawn_blocking` to avoid
/// blocking the async runtime with synchronous rusqlite calls.
#[derive(Debug, Clone)]
pub struct AsyncDatabase {
    inner: Arc<Mutex<Database>>,
}

impl AsyncDatabase {
    /// Creates a new async wrapper around a database.
    pub fn new(db: Database) -> Self {
        Self {
            inner: Arc::new(Mutex::new(db)),
        }
    }

    /// Saves a game snapshot.
    pub async fn save_snapshot(
        &self,
        branch_id: i64,
        snapshot: &GameSnapshot,
    ) -> Result<i64, ParishError> {
        let db = self.inner.clone();
        let snapshot = snapshot.clone();
        tokio::task::spawn_blocking(move || {
            let db = lock_recovered(&db);
            db.save_snapshot(branch_id, &snapshot)
        })
        .await
        .map_err(|e| ParishError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(e))))?
    }

    /// Loads the most recent snapshot for a branch.
    pub async fn load_latest_snapshot(
        &self,
        branch_id: i64,
    ) -> Result<Option<(i64, GameSnapshot)>, ParishError> {
        let db = self.inner.clone();
        tokio::task::spawn_blocking(
            move || -> Result<Option<(i64, GameSnapshot)>, ParishError> {
                let db = lock_recovered(&db);
                db.load_latest_snapshot(branch_id)
            },
        )
        .await
        .map_err(|e| ParishError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(e))))?
    }

    /// Creates a new branch.
    pub async fn create_branch(
        &self,
        name: &str,
        parent_branch_id: Option<i64>,
    ) -> Result<i64, ParishError> {
        let db = self.inner.clone();
        let name = name.to_string();
        tokio::task::spawn_blocking(move || {
            let db = lock_recovered(&db);
            db.create_branch(&name, parent_branch_id)
        })
        .await
        .map_err(|e| ParishError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(e))))?
    }

    /// Finds a branch by name.
    pub async fn find_branch(&self, name: &str) -> Result<Option<BranchInfo>, ParishError> {
        let db = self.inner.clone();
        let name = name.to_string();
        tokio::task::spawn_blocking(move || {
            let db = lock_recovered(&db);
            db.find_branch(&name)
        })
        .await
        .map_err(|e| ParishError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(e))))?
    }

    /// Lists all branches.
    pub async fn list_branches(&self) -> Result<Vec<BranchInfo>, ParishError> {
        let db = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            let db = lock_recovered(&db);
            db.list_branches()
        })
        .await
        .map_err(|e| ParishError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(e))))?
    }

    /// Appends a journal event.
    pub async fn append_event(
        &self,
        branch_id: i64,
        snapshot_id: i64,
        event: &WorldEvent,
        game_time: &str,
    ) -> Result<(), ParishError> {
        let db = self.inner.clone();
        let event = event.clone();
        let game_time = game_time.to_string();
        tokio::task::spawn_blocking(move || {
            let db = lock_recovered(&db);
            db.append_event(branch_id, snapshot_id, &event, &game_time)
        })
        .await
        .map_err(|e| ParishError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(e))))?
    }

    /// Returns events since a snapshot.
    pub async fn events_since_snapshot(
        &self,
        branch_id: i64,
        snapshot_id: i64,
    ) -> Result<Vec<WorldEvent>, ParishError> {
        let db = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            let db = lock_recovered(&db);
            db.events_since_snapshot(branch_id, snapshot_id)
        })
        .await
        .map_err(|e| ParishError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(e))))?
    }

    /// Returns the journal event count.
    pub async fn journal_count(
        &self,
        branch_id: i64,
        snapshot_id: i64,
    ) -> Result<usize, ParishError> {
        let db = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            let db = lock_recovered(&db);
            db.journal_count(branch_id, snapshot_id)
        })
        .await
        .map_err(|e| ParishError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(e))))?
    }

    /// Returns snapshot history for a branch.
    pub async fn branch_log(&self, branch_id: i64) -> Result<Vec<SnapshotInfo>, ParishError> {
        let db = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            let db = lock_recovered(&db);
            db.branch_log(branch_id)
        })
        .await
        .map_err(|e| ParishError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(e))))?
    }

    /// Clears journal events after a snapshot.
    pub async fn clear_journal(&self, branch_id: i64, snapshot_id: i64) -> Result<(), ParishError> {
        let db = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            let db = lock_recovered(&db);
            db.clear_journal(branch_id, snapshot_id)
        })
        .await
        .map_err(|e| ParishError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(e))))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::{ClockSnapshot, GameSnapshot};
    use chrono::{TimeZone, Utc};

    fn make_test_snapshot() -> GameSnapshot {
        GameSnapshot {
            player_location: parish_types::LocationId(1),
            weather: "Clear".to_string(),
            text_log: vec!["Hello".to_string()],
            clock: ClockSnapshot {
                game_time: Utc.with_ymd_and_hms(1820, 3, 20, 8, 0, 0).unwrap(),
                speed_factor: 36.0,
                paused: false,
            },
            npcs: Vec::new(),
            last_tier2_game_time: None,
            last_tier3_game_time: None,
            last_tier4_game_time: None,
            introduced_npcs: Default::default(),
            visited_locations: std::collections::HashSet::from([parish_types::LocationId(1)]),
            edge_traversals: Default::default(),
            gossip_network: Default::default(),
            conversation_log: Default::default(),
            player_name: None,
            npcs_who_know_player_name: Default::default(),
            letter_book: Default::default(),
        }
    }

    #[test]
    fn test_database_open_memory() {
        let db = Database::open_memory().unwrap();
        let branches = db.list_branches().unwrap();
        assert_eq!(branches.len(), 1);
        assert_eq!(branches[0].name, "main");
    }

    #[test]
    fn test_snapshot_save_and_load() {
        let db = Database::open_memory().unwrap();
        let branch = db.find_branch("main").unwrap().unwrap();
        let snapshot = make_test_snapshot();

        let snap_id = db.save_snapshot(branch.id, &snapshot).unwrap();
        assert!(snap_id > 0);

        let loaded = db.load_latest_snapshot(branch.id).unwrap().unwrap();
        assert_eq!(loaded.0, snap_id);
        assert_eq!(loaded.1.player_location, snapshot.player_location);
        assert_eq!(loaded.1.weather, snapshot.weather);
    }

    #[test]
    fn test_load_latest_snapshot_none() {
        let db = Database::open_memory().unwrap();
        let branch = db.find_branch("main").unwrap().unwrap();
        let result = db.load_latest_snapshot(branch.id).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_create_and_find_branch() {
        let db = Database::open_memory().unwrap();
        let id = db.create_branch("test-branch", Some(1)).unwrap();
        let found = db.find_branch("test-branch").unwrap().unwrap();
        assert_eq!(found.id, id);
        assert_eq!(found.name, "test-branch");
        assert_eq!(found.parent_branch_id, Some(1));
    }

    #[test]
    fn test_find_branch_not_found() {
        let db = Database::open_memory().unwrap();
        let result = db.find_branch("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_list_branches() {
        let db = Database::open_memory().unwrap();
        db.create_branch("alpha", None).unwrap();
        db.create_branch("beta", None).unwrap();
        let branches = db.list_branches().unwrap();
        assert_eq!(branches.len(), 3); // main + alpha + beta
    }

    #[test]
    fn test_journal_append_and_query() {
        let db = Database::open_memory().unwrap();
        let branch = db.find_branch("main").unwrap().unwrap();
        let snap_id = db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();

        let event1 = WorldEvent::PlayerMoved {
            from: parish_types::LocationId(1),
            to: parish_types::LocationId(2),
            minutes: None,
        };
        let event2 = WorldEvent::WeatherChanged {
            new_weather: "Rain".to_string(),
        };

        db.append_event(branch.id, snap_id, &event1, "1820-03-20T08:00:00Z")
            .unwrap();
        db.append_event(branch.id, snap_id, &event2, "1820-03-20T09:00:00Z")
            .unwrap();

        let events = db.events_since_snapshot(branch.id, snap_id).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0], event1);
        assert_eq!(events[1], event2);
    }

    #[test]
    fn test_journal_count() {
        let db = Database::open_memory().unwrap();
        let branch = db.find_branch("main").unwrap().unwrap();
        let snap_id = db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();

        assert_eq!(db.journal_count(branch.id, snap_id).unwrap(), 0);

        let event = WorldEvent::ClockAdvanced { minutes: 10 };
        db.append_event(branch.id, snap_id, &event, "1820-03-20T08:00:00Z")
            .unwrap();
        assert_eq!(db.journal_count(branch.id, snap_id).unwrap(), 1);
    }

    #[test]
    fn test_branch_log() {
        let db = Database::open_memory().unwrap();
        let branch = db.find_branch("main").unwrap().unwrap();

        db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();
        db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();

        let log = db.branch_log(branch.id).unwrap();
        assert_eq!(log.len(), 2);
        // Most recent first
        assert!(log[0].id > log[1].id);
    }

    #[test]
    fn test_clear_journal() {
        let db = Database::open_memory().unwrap();
        let branch = db.find_branch("main").unwrap().unwrap();
        let snap_id = db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();

        let event = WorldEvent::ClockAdvanced { minutes: 10 };
        db.append_event(branch.id, snap_id, &event, "1820-03-20T08:00:00Z")
            .unwrap();
        assert_eq!(db.journal_count(branch.id, snap_id).unwrap(), 1);

        db.clear_journal(branch.id, snap_id).unwrap();
        assert_eq!(db.journal_count(branch.id, snap_id).unwrap(), 0);
    }

    /// Compaction must be scoped to the `(branch_id, snapshot_id)` pair.
    /// Events tied to an *earlier* snapshot on the same branch must survive
    /// a `clear_journal` of a *later* snapshot (and vice versa) — otherwise
    /// compaction would delete history it has no business touching.
    #[test]
    fn test_clear_journal_scoped_to_snapshot_id() {
        let db = Database::open_memory().unwrap();
        let branch = db.find_branch("main").unwrap().unwrap();

        // Two snapshots on the same branch, each with their own journal tail.
        let snap1 = db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();
        db.append_event(
            branch.id,
            snap1,
            &WorldEvent::ClockAdvanced { minutes: 5 },
            "1820-03-20T08:00:00Z",
        )
        .unwrap();
        db.append_event(
            branch.id,
            snap1,
            &WorldEvent::WeatherChanged {
                new_weather: "Fog".to_string(),
            },
            "1820-03-20T08:05:00Z",
        )
        .unwrap();

        let snap2 = db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();
        db.append_event(
            branch.id,
            snap2,
            &WorldEvent::ClockAdvanced { minutes: 10 },
            "1820-03-20T09:00:00Z",
        )
        .unwrap();

        assert_eq!(db.journal_count(branch.id, snap1).unwrap(), 2);
        assert_eq!(db.journal_count(branch.id, snap2).unwrap(), 1);

        // Pruning snap2's journal must leave snap1's events alone.
        db.clear_journal(branch.id, snap2).unwrap();
        assert_eq!(
            db.journal_count(branch.id, snap1).unwrap(),
            2,
            "pruning snap2 must not touch snap1's journal"
        );
        assert_eq!(db.journal_count(branch.id, snap2).unwrap(), 0);

        // Pruning snap1 now clears the remaining two events.
        db.clear_journal(branch.id, snap1).unwrap();
        assert_eq!(db.journal_count(branch.id, snap1).unwrap(), 0);
    }

    /// Compaction must be scoped to the `branch_id` as well: pruning the
    /// journal for branch A must NEVER touch branch B's events.
    #[test]
    fn test_clear_journal_scoped_to_branch_id() {
        let db = Database::open_memory().unwrap();
        let main = db.find_branch("main").unwrap().unwrap();
        let fork_id = db.create_branch("fork", Some(main.id)).unwrap();

        let main_snap = db.save_snapshot(main.id, &make_test_snapshot()).unwrap();
        let fork_snap = db.save_snapshot(fork_id, &make_test_snapshot()).unwrap();

        db.append_event(
            main.id,
            main_snap,
            &WorldEvent::ClockAdvanced { minutes: 1 },
            "1820-03-20T08:00:00Z",
        )
        .unwrap();
        db.append_event(
            fork_id,
            fork_snap,
            &WorldEvent::ClockAdvanced { minutes: 2 },
            "1820-03-20T08:00:00Z",
        )
        .unwrap();

        db.clear_journal(main.id, main_snap).unwrap();

        assert_eq!(db.journal_count(main.id, main_snap).unwrap(), 0);
        assert_eq!(
            db.journal_count(fork_id, fork_snap).unwrap(),
            1,
            "pruning main's journal must not touch the fork"
        );
    }

    /// End-to-end compaction workflow:
    ///   1. save snapshot A
    ///   2. append N journal events after A
    ///   3. save snapshot B (which captures the state produced by those events)
    ///   4. clear_journal(A) — the tail is now redundant
    ///   5. load_latest_snapshot returns B, and events_since_snapshot(B) is empty
    ///
    /// This is the exact lifecycle the CLI / server paths drive via `/save`.
    #[test]
    fn test_compaction_lifecycle_matches_save_path() {
        let db = Database::open_memory().unwrap();
        let branch = db.find_branch("main").unwrap().unwrap();

        let snap_a = db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();
        for i in 0..4 {
            db.append_event(
                branch.id,
                snap_a,
                &WorldEvent::ClockAdvanced { minutes: i + 1 },
                "1820-03-20T08:00:00Z",
            )
            .unwrap();
        }
        assert_eq!(db.journal_count(branch.id, snap_a).unwrap(), 4);

        // New snapshot captures the post-event state, old journal pruned.
        let snap_b = db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();
        db.clear_journal(branch.id, snap_a).unwrap();

        assert_eq!(db.journal_count(branch.id, snap_a).unwrap(), 0);
        assert_eq!(db.journal_count(branch.id, snap_b).unwrap(), 0);

        // Loading the latest snapshot gives us snap_b and no tail.
        let (loaded_id, _) = db.load_latest_snapshot(branch.id).unwrap().unwrap();
        assert_eq!(loaded_id, snap_b);
        let tail = db.events_since_snapshot(branch.id, snap_b).unwrap();
        assert!(tail.is_empty());
    }

    /// Regression: `clear_journal` with zero events must succeed silently —
    /// the first-ever `/save` call on a fresh branch hits this path, since
    /// `latest_snapshot_id` may still point at the bootstrap snapshot whose
    /// journal tail is empty.
    #[test]
    fn test_clear_journal_on_empty_tail_is_noop() {
        let db = Database::open_memory().unwrap();
        let branch = db.find_branch("main").unwrap().unwrap();
        let snap_id = db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();

        assert_eq!(db.journal_count(branch.id, snap_id).unwrap(), 0);
        db.clear_journal(branch.id, snap_id).unwrap();
        assert_eq!(db.journal_count(branch.id, snap_id).unwrap(), 0);
    }

    #[test]
    fn test_multiple_snapshots_loads_latest() {
        let db = Database::open_memory().unwrap();
        let branch = db.find_branch("main").unwrap().unwrap();

        let mut snap1 = make_test_snapshot();
        snap1.weather = "Cloudy".to_string();
        db.save_snapshot(branch.id, &snap1).unwrap();

        let mut snap2 = make_test_snapshot();
        snap2.weather = "Sunny".to_string();
        let id2 = db.save_snapshot(branch.id, &snap2).unwrap();

        let (loaded_id, loaded) = db.load_latest_snapshot(branch.id).unwrap().unwrap();
        assert_eq!(loaded_id, id2);
        assert_eq!(loaded.weather, "Sunny");
    }

    #[test]
    fn test_journal_sequence_ordering() {
        let db = Database::open_memory().unwrap();
        let branch = db.find_branch("main").unwrap().unwrap();
        let snap_id = db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();

        // Append 5 events and verify ordering
        for i in 0..5 {
            let event = WorldEvent::ClockAdvanced { minutes: i + 1 };
            db.append_event(branch.id, snap_id, &event, "1820-03-20T08:00:00Z")
                .unwrap();
        }

        let events = db.events_since_snapshot(branch.id, snap_id).unwrap();
        assert_eq!(events.len(), 5);
        for (i, event) in events.iter().enumerate() {
            match event {
                WorldEvent::ClockAdvanced { minutes } => {
                    assert_eq!(*minutes, (i as i64) + 1);
                }
                _ => panic!("unexpected event type"),
            }
        }
    }

    #[test]
    fn test_open_file_database() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let db = Database::open(tmp.path()).unwrap();
        let branches = db.list_branches().unwrap();
        assert_eq!(branches.len(), 1);
        assert_eq!(branches[0].name, "main");
    }

    #[test]
    fn test_duplicate_branch_name_fails() {
        let db = Database::open_memory().unwrap();
        db.create_branch("test", None).unwrap();
        let result = db.create_branch("test", None);
        assert!(result.is_err());
    }

    // --- AsyncDatabase wrapper ---

    #[tokio::test]
    async fn test_async_save_and_load_roundtrip() {
        let db = Database::open_memory().unwrap();
        let async_db = AsyncDatabase::new(db);

        let branch = async_db.find_branch("main").await.unwrap().unwrap();
        let snap = make_test_snapshot();
        let snap_id = async_db.save_snapshot(branch.id, &snap).await.unwrap();

        let loaded = async_db
            .load_latest_snapshot(branch.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded.0, snap_id);
        assert_eq!(loaded.1.player_location, snap.player_location);
        assert_eq!(loaded.1.weather, snap.weather);
    }

    #[tokio::test]
    async fn test_async_branch_crud() {
        let db = Database::open_memory().unwrap();
        let async_db = AsyncDatabase::new(db);

        let fork_id = async_db.create_branch("fork", None).await.unwrap();
        let found = async_db.find_branch("fork").await.unwrap();
        assert_eq!(found.unwrap().id, fork_id);

        let branches = async_db.list_branches().await.unwrap();
        assert_eq!(branches.len(), 2); // main + fork
    }

    #[tokio::test]
    async fn test_async_journal_append_and_count() {
        let db = Database::open_memory().unwrap();
        let async_db = AsyncDatabase::new(db);

        let branch = async_db.find_branch("main").await.unwrap().unwrap();
        let snap_id = async_db
            .save_snapshot(branch.id, &make_test_snapshot())
            .await
            .unwrap();

        async_db
            .append_event(
                branch.id,
                snap_id,
                &WorldEvent::ClockAdvanced { minutes: 5 },
                "1820-03-20T08:00:00Z",
            )
            .await
            .unwrap();

        let count = async_db.journal_count(branch.id, snap_id).await.unwrap();
        assert_eq!(count, 1);

        let events = async_db
            .events_since_snapshot(branch.id, snap_id)
            .await
            .unwrap();
        assert_eq!(events.len(), 1);
    }

    #[tokio::test]
    async fn test_async_clear_journal() {
        let db = Database::open_memory().unwrap();
        let async_db = AsyncDatabase::new(db);

        let branch = async_db.find_branch("main").await.unwrap().unwrap();
        let snap_id = async_db
            .save_snapshot(branch.id, &make_test_snapshot())
            .await
            .unwrap();

        async_db
            .append_event(
                branch.id,
                snap_id,
                &WorldEvent::ClockAdvanced { minutes: 5 },
                "1820-03-20T08:00:00Z",
            )
            .await
            .unwrap();

        async_db.clear_journal(branch.id, snap_id).await.unwrap();
        let count = async_db.journal_count(branch.id, snap_id).await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_async_branch_log() {
        let db = Database::open_memory().unwrap();
        let async_db = AsyncDatabase::new(db);

        let branch = async_db.find_branch("main").await.unwrap().unwrap();
        async_db
            .save_snapshot(branch.id, &make_test_snapshot())
            .await
            .unwrap();
        async_db
            .save_snapshot(branch.id, &make_test_snapshot())
            .await
            .unwrap();

        let log = async_db.branch_log(branch.id).await.unwrap();
        assert_eq!(log.len(), 2);
    }

    // --- Issue #225: PRAGMA foreign_keys=ON enforcement ---

    #[test]
    fn test_foreign_key_snapshot_references_branch() {
        let db = Database::open_memory().unwrap();
        let snapshot = make_test_snapshot();
        // branch_id 999 does not exist; FK enforcement must reject the insert.
        let result = db.save_snapshot(999, &snapshot);
        assert!(
            result.is_err(),
            "save_snapshot with a non-existent branch_id should fail with FK violation"
        );
    }

    #[test]
    fn test_foreign_key_journal_references_snapshot() {
        let db = Database::open_memory().unwrap();
        let branch = db.find_branch("main").unwrap().unwrap();
        let event = WorldEvent::ClockAdvanced { minutes: 1 };
        // snapshot_id 999 does not exist; FK enforcement must reject the insert.
        let result = db.append_event(branch.id, 999, &event, "1820-03-20T08:00:00Z");
        assert!(
            result.is_err(),
            "append_event with a non-existent snapshot_id should fail with FK violation"
        );
    }

    // --- Issue #226: atomic sequence + UNIQUE constraint ---

    #[test]
    fn test_append_event_sequence_starts_at_one() {
        let db = Database::open_memory().unwrap();
        let branch = db.find_branch("main").unwrap().unwrap();
        let snap_id = db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();

        let event = WorldEvent::ClockAdvanced { minutes: 5 };
        db.append_event(branch.id, snap_id, &event, "1820-03-20T08:00:00Z")
            .unwrap();

        // Verify sequence=1 was assigned by querying the count (sequence numbers
        // are internal, but correct ordering is observable via events_since_snapshot).
        let events = db.events_since_snapshot(branch.id, snap_id).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], event);
    }

    #[test]
    fn test_append_event_sequences_are_contiguous() {
        let db = Database::open_memory().unwrap();
        let branch = db.find_branch("main").unwrap().unwrap();
        let snap_id = db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();

        for i in 1..=10i64 {
            let event = WorldEvent::ClockAdvanced { minutes: i };
            db.append_event(branch.id, snap_id, &event, "1820-03-20T08:00:00Z")
                .unwrap();
        }

        let events = db.events_since_snapshot(branch.id, snap_id).unwrap();
        assert_eq!(events.len(), 10);
        // Events must arrive in insertion order (ascending sequence).
        for (idx, ev) in events.iter().enumerate() {
            match ev {
                WorldEvent::ClockAdvanced { minutes } => {
                    assert_eq!(*minutes, (idx as i64) + 1);
                }
                _ => panic!("unexpected event type"),
            }
        }
    }

    #[test]
    fn test_sequences_are_independent_per_snapshot() {
        // Each snapshot has its own sequence counter starting from 1.
        let db = Database::open_memory().unwrap();
        let branch = db.find_branch("main").unwrap().unwrap();

        let snap1 = db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();
        let snap2 = db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();

        let ev = WorldEvent::ClockAdvanced { minutes: 1 };
        db.append_event(branch.id, snap1, &ev, "1820-03-20T08:00:00Z")
            .unwrap();
        db.append_event(branch.id, snap2, &ev, "1820-03-20T08:00:00Z")
            .unwrap();

        // Both snapshots should have exactly one event each.
        assert_eq!(db.journal_count(branch.id, snap1).unwrap(), 1);
        assert_eq!(db.journal_count(branch.id, snap2).unwrap(), 1);
    }

    /// Regression test for #82: a previous panic while holding the mutex
    /// must not cascade into every subsequent database call panicking.
    /// `lock_recovered` should transparently recover from the poisoned
    /// state.
    #[test]
    fn test_lock_recovered_handles_poisoned_mutex() {
        let mutex: Arc<Mutex<u32>> = Arc::new(Mutex::new(42));

        // Poison the mutex by panicking in another thread while holding it.
        let mutex_clone = mutex.clone();
        let _ = std::thread::spawn(move || {
            let _guard = mutex_clone.lock().unwrap();
            panic!("intentional panic to poison the mutex");
        })
        .join();

        // The mutex is now poisoned; `.lock()` returns `Err(_)`.
        assert!(mutex.lock().is_err(), "mutex should be poisoned");

        // `lock_recovered` must still yield the inner value.
        let guard = lock_recovered(&mutex);
        assert_eq!(*guard, 42);
    }

    /// Regression test for #82: poison recovery also works on the
    /// `Arc<Mutex<Database>>` used inside `AsyncDatabase`.
    #[test]
    fn test_lock_recovered_with_database_after_poison() {
        let db = Database::open_memory().unwrap();
        let branch = db.find_branch("main").unwrap().unwrap();
        let inner: Arc<Mutex<Database>> = Arc::new(Mutex::new(db));

        // Poison the inner mutex.
        let inner_clone = inner.clone();
        let _ = std::thread::spawn(move || {
            let _guard = inner_clone.lock().unwrap();
            panic!("intentional panic");
        })
        .join();

        assert!(inner.lock().is_err(), "database mutex should be poisoned");

        // Recover and verify the database is still usable.
        let db = lock_recovered(&inner);
        let loaded = db
            .find_branch(&branch.name)
            .expect("find_branch should still work after poison recovery");
        assert!(loaded.is_some());
    }
}
