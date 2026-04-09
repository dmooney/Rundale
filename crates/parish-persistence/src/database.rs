//! SQLite database layer for persistence.
//!
//! Provides [`Database`] (synchronous) and [`AsyncDatabase`] (async wrapper)
//! for managing branches, snapshots, and journal events. Uses WAL mode for
//! concurrent read/write access.

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{Connection, OptionalExtension, params};

use crate::journal::WorldEvent;
use crate::snapshot::GameSnapshot;
use parish_types::ParishError;

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
    /// performance, then runs migrations to ensure the schema is current.
    pub fn open(path: &Path) -> Result<Self, ParishError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;",
        )?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    /// Opens an in-memory database (for testing).
    pub fn open_memory() -> Result<Self, ParishError> {
        let conn = Connection::open_in_memory()?;
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

            CREATE INDEX IF NOT EXISTS idx_journal_branch_snap_seq
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
    /// The sequence number is auto-incremented within the (branch, snapshot) scope.
    pub fn append_event(
        &self,
        branch_id: i64,
        snapshot_id: i64,
        event: &WorldEvent,
        game_time: &str,
    ) -> Result<(), ParishError> {
        let sequence: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(sequence), 0) + 1
             FROM journal_events
             WHERE branch_id = ?1 AND after_snapshot_id = ?2",
            params![branch_id, snapshot_id],
            |row| row.get(0),
        )?;

        let event_data = serde_json::to_string(event)?;
        let event_type = event.event_type();

        self.conn.execute(
            "INSERT INTO journal_events
             (branch_id, sequence, after_snapshot_id, event_type, event_data, game_time)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                branch_id,
                sequence,
                snapshot_id,
                event_type,
                event_data,
                game_time
            ],
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
            let db = db.lock().expect("database lock poisoned");
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
                let db = db.lock().expect("database lock poisoned");
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
            let db = db.lock().expect("database lock poisoned");
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
            let db = db.lock().expect("database lock poisoned");
            db.find_branch(&name)
        })
        .await
        .map_err(|e| ParishError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(e))))?
    }

    /// Lists all branches.
    pub async fn list_branches(&self) -> Result<Vec<BranchInfo>, ParishError> {
        let db = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            let db = db.lock().expect("database lock poisoned");
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
            let db = db.lock().expect("database lock poisoned");
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
            let db = db.lock().expect("database lock poisoned");
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
            let db = db.lock().expect("database lock poisoned");
            db.journal_count(branch_id, snapshot_id)
        })
        .await
        .map_err(|e| ParishError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(e))))?
    }

    /// Returns snapshot history for a branch.
    pub async fn branch_log(&self, branch_id: i64) -> Result<Vec<SnapshotInfo>, ParishError> {
        let db = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            let db = db.lock().expect("database lock poisoned");
            db.branch_log(branch_id)
        })
        .await
        .map_err(|e| ParishError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(e))))?
    }

    /// Clears journal events after a snapshot.
    pub async fn clear_journal(&self, branch_id: i64, snapshot_id: i64) -> Result<(), ParishError> {
        let db = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            let db = db.lock().expect("database lock poisoned");
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
            visited_locations: std::collections::HashSet::from([parish_types::LocationId(1)]),
            edge_traversals: Default::default(),
            gossip_network: Default::default(),
            conversation_log: Default::default(),
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
}
