//! Request and transcript tables of the turn journal.
//!
//! A save keeps the requests the player submitted (`requests`) and the
//! transcript events those requests produced (`transcript_events`) next to
//! its snapshots and world-event journal. Both are scoped to a branch: a
//! request belongs to the branch it was played on, and forking a branch
//! copies neither table (the fork starts its own turn history).
//!
//! The rows hold opaque JSON written and validated by the turn engine's
//! SQLite journal in `limerick-core`; this module owns only the storage:
//! the schema, one write transaction per journal call, and ordered reads.
//!
//! `transcript_events.sequence` is an `AUTOINCREMENT` key, so SQLite's
//! durable sequence counter assigns it: it strictly increases across the
//! whole save and is never reused, even after a rolled-back write or a
//! deleted branch. `event_id` is unique across the save.

use rusqlite::{Connection, OptionalExtension as _, Transaction, TransactionBehavior, params};

use crate::IntoLimerickDbError as _;
use crate::journal::WorldEvent;
use crate::snapshot::GameSnapshot;
use limerick_types::LimerickError;

/// Creates the turn-journal tables when they are missing.
///
/// Saves written before the turn journal existed gain empty tables the
/// first time they are opened; nothing else in them changes.
pub(super) fn migrate(conn: &Connection) -> Result<(), LimerickError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS requests (
            request_id TEXT PRIMARY KEY,
            branch_id INTEGER NOT NULL REFERENCES branches(id) ON DELETE CASCADE,
            phase TEXT NOT NULL,
            is_open INTEGER NOT NULL,
            committed_revision INTEGER,
            record TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_requests_branch_open
            ON requests(branch_id, is_open);

        CREATE TABLE IF NOT EXISTS transcript_events (
            sequence INTEGER PRIMARY KEY AUTOINCREMENT,
            event_id TEXT NOT NULL,
            branch_id INTEGER NOT NULL REFERENCES branches(id) ON DELETE CASCADE,
            request_id TEXT,
            kind TEXT NOT NULL,
            event TEXT NOT NULL
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_transcript_events_event_id
            ON transcript_events(event_id);
        CREATE INDEX IF NOT EXISTS idx_transcript_events_branch_sequence
            ON transcript_events(branch_id, sequence);",
    )
    .db_err()
}

/// One row of the `requests` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnRequestRow {
    /// Logical request id.
    pub request_id: String,
    /// Branch the request was played on.
    pub branch_id: i64,
    /// Lifecycle phase name (for queries and inspection).
    pub phase: String,
    /// Whether the request is not yet terminal.
    pub is_open: bool,
    /// Revision the request committed, once it succeeded.
    pub committed_revision: Option<u64>,
    /// The serialized request record.
    pub record: String,
}

/// One row of the `transcript_events` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptEventRow {
    /// Save-wide sequence, assigned at insert.
    pub sequence: u64,
    /// Event id, unique across the save.
    pub event_id: String,
    /// Branch the event was played on.
    pub branch_id: i64,
    /// Owning request, when the event belongs to one.
    pub request_id: Option<String>,
    /// Event kind name.
    pub kind: String,
    /// The serialized event (without its sequence).
    pub event: String,
}

/// A new transcript event, before its sequence is assigned.
#[derive(Debug, Clone, Copy)]
pub struct NewTranscriptEvent<'a> {
    /// Event id.
    pub event_id: &'a str,
    /// Owning request.
    pub request_id: Option<&'a str>,
    /// Event kind name.
    pub kind: &'a str,
    /// The serialized event.
    pub event: &'a str,
}

/// The writes of one turn-journal call, inside one SQLite transaction.
///
/// Obtained from [`crate::Database::turn_journal_transaction`]. Every write
/// made through it commits together when the closure returns `Ok`, and none
/// of them does when it returns `Err` or a statement fails.
pub struct TurnJournalWriter<'t> {
    tx: &'t Transaction<'t>,
    now: &'t str,
}

impl TurnJournalWriter<'_> {
    /// The stored request `request_id`, on any branch.
    pub fn request(&self, request_id: &str) -> Result<Option<TurnRequestRow>, LimerickError> {
        request(self.tx, request_id)
    }

    /// The stored event `event_id`, on any branch.
    pub fn event(&self, event_id: &str) -> Result<Option<TranscriptEventRow>, LimerickError> {
        self.tx
            .query_row(
                "SELECT sequence, event_id, branch_id, request_id, kind, event
                 FROM transcript_events WHERE event_id = ?1",
                params![event_id],
                event_row,
            )
            .optional()
            .db_err()
    }

    /// Inserts or replaces the row of `row.request_id`.
    pub fn put_request(&self, row: &TurnRequestRow) -> Result<(), LimerickError> {
        let revision = row
            .committed_revision
            .map(i64::try_from)
            .transpose()
            .map_err(|_| LimerickError::Database("committed revision out of range".into()))?;
        self.tx
            .execute(
                "INSERT INTO requests
                     (request_id, branch_id, phase, is_open, committed_revision, record)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(request_id) DO UPDATE SET
                     branch_id = excluded.branch_id,
                     phase = excluded.phase,
                     is_open = excluded.is_open,
                     committed_revision = excluded.committed_revision,
                     record = excluded.record",
                params![
                    row.request_id,
                    row.branch_id,
                    row.phase,
                    row.is_open,
                    revision,
                    row.record
                ],
            )
            .db_err()?;
        Ok(())
    }

    /// Appends a transcript event to `branch_id` and returns the sequence
    /// the save's counter assigned it.
    pub fn append_event(
        &self,
        branch_id: i64,
        event: NewTranscriptEvent<'_>,
    ) -> Result<u64, LimerickError> {
        self.tx
            .execute(
                "INSERT INTO transcript_events (event_id, branch_id, request_id, kind, event)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    event.event_id,
                    branch_id,
                    event.request_id,
                    event.kind,
                    event.event
                ],
            )
            .db_err()?;
        sequence(self.tx.last_insert_rowid())
    }

    /// Saves `snapshot` as the latest snapshot of `branch_id` and returns
    /// its row id.
    pub fn save_snapshot(
        &self,
        branch_id: i64,
        snapshot: &GameSnapshot,
    ) -> Result<i64, LimerickError> {
        super::journal::save_snapshot(self.tx, branch_id, snapshot, self.now)
    }

    /// Appends world events to snapshot `snapshot_id` of `branch_id`.
    pub fn append_world_events(
        &self,
        branch_id: i64,
        snapshot_id: i64,
        events: &[(WorldEvent, String)],
    ) -> Result<(), LimerickError> {
        for (event, game_time) in events {
            super::journal::append_event(self.tx, branch_id, snapshot_id, event, game_time)?;
        }
        Ok(())
    }

    /// The latest snapshot of `branch_id`, if it has one.
    pub fn latest_snapshot_id(&self, branch_id: i64) -> Result<Option<i64>, LimerickError> {
        super::journal::latest_snapshot_id(self.tx, branch_id)
    }
}

/// Runs `write` in one immediate write transaction, committing when it
/// returns `Ok`. An error from `write` or from the commit rolls everything
/// back.
pub(super) fn transaction<T, E>(
    conn: &Connection,
    now: &str,
    write: impl FnOnce(&TurnJournalWriter<'_>) -> Result<T, E>,
) -> Result<T, E>
where
    E: From<LimerickError>,
{
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate).db_err()?;
    let value = write(&TurnJournalWriter { tx: &tx, now })?;
    tx.commit().db_err()?;
    Ok(value)
}

/// The stored request `request_id`, on any branch.
pub(super) fn request(
    conn: &Connection,
    request_id: &str,
) -> Result<Option<TurnRequestRow>, LimerickError> {
    conn.query_row(
        "SELECT request_id, branch_id, phase, is_open, committed_revision, record
         FROM requests WHERE request_id = ?1",
        params![request_id],
        request_row,
    )
    .optional()
    .db_err()
}

/// Every request of `branch_id`, in insertion order.
pub(super) fn requests(
    conn: &Connection,
    branch_id: i64,
    open_only: bool,
) -> Result<Vec<TurnRequestRow>, LimerickError> {
    let mut stmt = conn
        .prepare(
            "SELECT request_id, branch_id, phase, is_open, committed_revision, record
             FROM requests
             WHERE branch_id = ?1 AND (?2 = 0 OR is_open = 1)
             ORDER BY rowid ASC",
        )
        .db_err()?;
    let rows = stmt
        .query_map(params![branch_id, open_only], request_row)
        .db_err()?;
    rows.collect::<Result<Vec<_>, _>>().db_err()
}

/// Events of `branch_id` with a sequence above `after`, in sequence order.
pub(super) fn events(
    conn: &Connection,
    branch_id: i64,
    after: u64,
) -> Result<Vec<TranscriptEventRow>, LimerickError> {
    let after = i64::try_from(after)
        .map_err(|_| LimerickError::Database("event sequence out of range".into()))?;
    let mut stmt = conn
        .prepare(
            "SELECT sequence, event_id, branch_id, request_id, kind, event
             FROM transcript_events
             WHERE branch_id = ?1 AND sequence > ?2
             ORDER BY sequence ASC",
        )
        .db_err()?;
    let rows = stmt
        .query_map(params![branch_id, after], event_row)
        .db_err()?;
    rows.collect::<Result<Vec<_>, _>>().db_err()
}

/// At most `limit` events of `branch_id` with a sequence above `after`, in
/// sequence order.
pub(super) fn events_page(
    conn: &Connection,
    branch_id: i64,
    after: u64,
    limit: usize,
) -> Result<Vec<TranscriptEventRow>, LimerickError> {
    let after = i64::try_from(after)
        .map_err(|_| LimerickError::Database("event sequence out of range".into()))?;
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let mut stmt = conn
        .prepare(
            "SELECT sequence, event_id, branch_id, request_id, kind, event
             FROM transcript_events
             WHERE branch_id = ?1 AND sequence > ?2
             ORDER BY sequence ASC
             LIMIT ?3",
        )
        .db_err()?;
    let rows = stmt
        .query_map(params![branch_id, after, limit], event_row)
        .db_err()?;
    rows.collect::<Result<Vec<_>, _>>().db_err()
}

/// The newest `limit` events of `branch_id` with a sequence below `before`,
/// in sequence order.
pub(super) fn events_before(
    conn: &Connection,
    branch_id: i64,
    before: u64,
    limit: usize,
) -> Result<Vec<TranscriptEventRow>, LimerickError> {
    let before = i64::try_from(before).unwrap_or(i64::MAX);
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let mut stmt = conn
        .prepare(
            "SELECT sequence, event_id, branch_id, request_id, kind, event
             FROM transcript_events
             WHERE branch_id = ?1 AND sequence < ?2
             ORDER BY sequence DESC
             LIMIT ?3",
        )
        .db_err()?;
    let rows = stmt
        .query_map(params![branch_id, before, limit], event_row)
        .db_err()?;
    let mut rows = rows.collect::<Result<Vec<_>, _>>().db_err()?;
    rows.reverse();
    Ok(rows)
}

fn sequence(raw: i64) -> Result<u64, LimerickError> {
    u64::try_from(raw).map_err(|_| LimerickError::Database("negative event sequence".into()))
}

fn request_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TurnRequestRow> {
    let revision: Option<i64> = row.get(4)?;
    Ok(TurnRequestRow {
        request_id: row.get(0)?,
        branch_id: row.get(1)?,
        phase: row.get(2)?,
        is_open: row.get(3)?,
        committed_revision: revision.and_then(|value| u64::try_from(value).ok()),
        record: row.get(5)?,
    })
}

fn event_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TranscriptEventRow> {
    let sequence: i64 = row.get(0)?;
    Ok(TranscriptEventRow {
        sequence: u64::try_from(sequence).unwrap_or_default(),
        event_id: row.get(1)?,
        branch_id: row.get(2)?,
        request_id: row.get(3)?,
        kind: row.get(4)?,
        event: row.get(5)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Database;

    fn snapshot() -> GameSnapshot {
        GameSnapshot::capture(
            &limerick_world::WorldState::new(),
            &limerick_npc::manager::NpcManager::new(),
        )
    }

    fn request(id: &str, branch_id: i64) -> TurnRequestRow {
        TurnRequestRow {
            request_id: id.to_string(),
            branch_id,
            phase: "accepted".to_string(),
            is_open: true,
            committed_revision: None,
            record: format!("{{\"id\":\"{id}\"}}"),
        }
    }

    fn event<'a>(id: &'a str, request: &'a str) -> NewTranscriptEvent<'a> {
        NewTranscriptEvent {
            event_id: id,
            request_id: Some(request),
            kind: "player_command",
            event: "{}",
        }
    }

    fn counts(db: &Database) -> (i64, i64, i64, i64) {
        let count = |table: &str| -> i64 {
            db.conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap()
        };
        (
            count("requests"),
            count("transcript_events"),
            count("snapshots"),
            count("journal_events"),
        )
    }

    #[test]
    fn opening_twice_keeps_the_tables_and_their_rows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("save.db");
        let main = {
            let db = Database::open(&path).unwrap();
            let main = db.find_branch("main").unwrap().unwrap().id;
            db.turn_journal_transaction(|writer| {
                writer.put_request(&request("r1", main))?;
                writer.append_event(main, event("e1", "r1")).map(|_| ())
            })
            .unwrap();
            main
        };
        let db = Database::open(&path).unwrap();
        assert_eq!(db.turn_requests(main).unwrap().len(), 1);
        assert_eq!(db.transcript_events(main, 0).unwrap().len(), 1);
    }

    #[test]
    fn a_failed_write_leaves_no_row_in_any_table() {
        let db = Database::open_memory().unwrap();
        let main = db.find_branch("main").unwrap().unwrap().id;
        db.save_snapshot(main, &snapshot()).unwrap();
        let before = counts(&db);

        let error: Result<(), LimerickError> = db.turn_journal_transaction(|writer| {
            writer.put_request(&request("r1", main))?;
            writer.append_event(main, event("e1", "r1"))?;
            let snapshot_id = writer.save_snapshot(main, &snapshot())?;
            writer.append_world_events(
                main,
                snapshot_id,
                &[(
                    WorldEvent::WeatherChanged {
                        new_weather: "Rain".to_string(),
                    },
                    "1820-03-20T08:00:00+00:00".to_string(),
                )],
            )?;
            Err(LimerickError::Database("injected".to_string()))
        });
        assert!(error.is_err());
        assert_eq!(counts(&db), before, "the whole write rolled back");
    }

    #[test]
    fn a_failing_statement_rolls_back_the_statements_before_it() {
        let db = Database::open_memory().unwrap();
        let main = db.find_branch("main").unwrap().unwrap().id;
        db.conn
            .execute_batch(
                "CREATE TRIGGER fail_snapshot BEFORE INSERT ON snapshots
                 BEGIN SELECT RAISE(ABORT, 'injected snapshot failure'); END;",
            )
            .unwrap();
        let before = counts(&db);
        let error = db
            .turn_journal_transaction(|writer| {
                writer.put_request(&request("r1", main))?;
                writer.append_event(main, event("e1", "r1"))?;
                writer.save_snapshot(main, &snapshot()).map(|_| ())
            })
            .unwrap_err();
        assert!(error.to_string().contains("injected snapshot failure"));
        assert_eq!(counts(&db), before);
    }

    #[test]
    fn sequences_strictly_increase_across_branches_and_are_never_reused() {
        let db = Database::open_memory().unwrap();
        let main = db.find_branch("main").unwrap().unwrap().id;
        let fork = db.create_branch("fork", Some(main)).unwrap();
        let first = db
            .turn_journal_transaction(|writer| writer.append_event(main, event("e1", "r1")))
            .unwrap();
        let _: Result<u64, LimerickError> = db.turn_journal_transaction(|writer| {
            writer.append_event(fork, event("e2", "r2"))?;
            Err(LimerickError::Database("rolled back".to_string()))
        });
        let second = db
            .turn_journal_transaction(|writer| writer.append_event(fork, event("e3", "r3")))
            .unwrap();
        let third = db
            .turn_journal_transaction(|writer| writer.append_event(main, event("e4", "r4")))
            .unwrap();
        assert!(first < second && second < third);
        let main_events = db.transcript_events(main, 0).unwrap();
        assert_eq!(
            main_events
                .iter()
                .map(|row| row.event_id.as_str())
                .collect::<Vec<_>>(),
            vec!["e1", "e4"],
            "events are scoped to their branch"
        );
        assert_eq!(db.transcript_events(main, first).unwrap().len(), 1);
    }

    #[test]
    fn an_event_id_is_unique_across_the_save() {
        let db = Database::open_memory().unwrap();
        let main = db.find_branch("main").unwrap().unwrap().id;
        let fork = db.create_branch("fork", Some(main)).unwrap();
        db.turn_journal_transaction(|writer| writer.append_event(main, event("e1", "r1")))
            .unwrap();
        assert!(
            db.turn_journal_transaction(|writer| writer.append_event(fork, event("e1", "r1")))
                .is_err()
        );
    }

    #[test]
    fn requests_update_in_place_and_open_requests_filter_terminal_ones() {
        let db = Database::open_memory().unwrap();
        let main = db.find_branch("main").unwrap().unwrap().id;
        db.turn_journal_transaction(|writer| {
            writer.put_request(&request("r1", main))?;
            writer.put_request(&request("r2", main))
        })
        .unwrap();
        let mut done = request("r1", main);
        done.phase = "completed".to_string();
        done.is_open = false;
        done.committed_revision = Some(3);
        db.turn_journal_transaction(|writer| writer.put_request(&done))
            .unwrap();
        let all = db.turn_requests(main).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0], done, "updated in place, first-accepted order kept");
        let open = db.open_turn_requests(main).unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].request_id, "r2");
    }

    #[test]
    fn deleting_a_branch_deletes_its_turn_rows() {
        let db = Database::open_memory().unwrap();
        let main = db.find_branch("main").unwrap().unwrap().id;
        let fork = db.create_branch("fork", Some(main)).unwrap();
        db.turn_journal_transaction(|writer| {
            writer.put_request(&request("r1", fork))?;
            writer.append_event(fork, event("e1", "r1")).map(|_| ())
        })
        .unwrap();
        db.delete_branch(fork).unwrap();
        assert_eq!(counts(&db).0, 0);
        assert_eq!(counts(&db).1, 0);
    }
}
