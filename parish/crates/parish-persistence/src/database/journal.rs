//! Journal and snapshot operations: append, query, compact, and branch log.

use rusqlite::{Connection, params};

use crate::IntoParishDbError as _;
use crate::journal::WorldEvent;
use crate::snapshot::GameSnapshot;
use parish_types::ParishError;

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

/// Saves a game snapshot to the given branch.
///
/// Returns the snapshot row id.
pub(super) fn save_snapshot(
    conn: &Connection,
    branch_id: i64,
    snapshot: &GameSnapshot,
) -> Result<i64, ParishError> {
    let world_state = serde_json::to_string(snapshot)?;
    let game_time = snapshot.clock.game_time.to_rfc3339();
    let real_time = chrono::Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO snapshots (branch_id, game_time, real_time, world_state)
         VALUES (?1, ?2, ?3, ?4)",
        params![branch_id, game_time, real_time, world_state],
    )
    .db_err()?;
    Ok(conn.last_insert_rowid())
}

/// Loads the most recent snapshot for a branch.
///
/// Returns `None` if no snapshots exist for the branch.
pub(super) fn load_latest_snapshot(
    conn: &Connection,
    branch_id: i64,
) -> Result<Option<(i64, GameSnapshot)>, ParishError> {
    use rusqlite::OptionalExtension as _;

    let result: Option<(i64, String)> = conn
        .query_row(
            "SELECT id, world_state FROM snapshots
             WHERE branch_id = ?1
             ORDER BY id DESC LIMIT 1",
            params![branch_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .db_err()?;

    match result {
        Some((id, json)) => {
            let snapshot: GameSnapshot = serde_json::from_str(&json)?;
            Ok(Some((id, snapshot)))
        }
        None => Ok(None),
    }
}

/// Appends a journal event for the given branch and snapshot.
///
/// The sequence number is computed and inserted atomically via a single
/// INSERT … SELECT statement, preventing duplicate sequences under
/// concurrent writes. The UNIQUE index on (branch_id, after_snapshot_id,
/// sequence) provides a second line of defence at the database level.
pub(super) fn append_event(
    conn: &Connection,
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
    conn.execute(
        "INSERT INTO journal_events
         (branch_id, sequence, after_snapshot_id, event_type, event_data, game_time)
         SELECT ?1, COALESCE(MAX(sequence), 0) + 1, ?2, ?3, ?4, ?5
         FROM journal_events
         WHERE branch_id = ?1 AND after_snapshot_id = ?2",
        params![branch_id, snapshot_id, event_type, event_data, game_time],
    )
    .db_err()?;
    Ok(())
}

/// Returns all journal events after a given snapshot for a branch.
pub(super) fn events_since_snapshot(
    conn: &Connection,
    branch_id: i64,
    snapshot_id: i64,
) -> Result<Vec<WorldEvent>, ParishError> {
    let mut stmt = conn
        .prepare(
            "SELECT event_data FROM journal_events
             WHERE branch_id = ?1 AND after_snapshot_id = ?2
             ORDER BY sequence ASC",
        )
        .db_err()?;
    let rows = stmt
        .query_map(params![branch_id, snapshot_id], |row| {
            let data: String = row.get(0)?;
            Ok(data)
        })
        .db_err()?;
    let mut events = Vec::new();
    for row in rows {
        let json = row.db_err()?;
        let event: WorldEvent = serde_json::from_str(&json)?;
        events.push(event);
    }
    Ok(events)
}

/// Returns the number of journal events after a given snapshot.
pub(super) fn journal_count(
    conn: &Connection,
    branch_id: i64,
    snapshot_id: i64,
) -> Result<usize, ParishError> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM journal_events
             WHERE branch_id = ?1 AND after_snapshot_id = ?2",
            params![branch_id, snapshot_id],
            |row| row.get(0),
        )
        .db_err()?;
    Ok(count as usize)
}

/// Returns snapshot history for a branch (most recent first).
pub(super) fn branch_log(
    conn: &Connection,
    branch_id: i64,
) -> Result<Vec<SnapshotInfo>, ParishError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, game_time, real_time FROM snapshots
             WHERE branch_id = ?1
             ORDER BY id DESC",
        )
        .db_err()?;
    let rows = stmt
        .query_map(params![branch_id], |row| {
            Ok(SnapshotInfo {
                id: row.get(0)?,
                game_time: row.get(1)?,
                real_time: row.get(2)?,
            })
        })
        .db_err()?;
    let mut infos = Vec::new();
    for row in rows {
        infos.push(row.db_err()?);
    }
    Ok(infos)
}

/// Deletes journal events for a branch after a given snapshot.
///
/// Used during compaction after a new snapshot is taken.
pub(super) fn clear_journal(
    conn: &Connection,
    branch_id: i64,
    snapshot_id: i64,
) -> Result<(), ParishError> {
    conn.execute(
        "DELETE FROM journal_events
         WHERE branch_id = ?1 AND after_snapshot_id = ?2",
        params![branch_id, snapshot_id],
    )
    .db_err()?;
    Ok(())
}
